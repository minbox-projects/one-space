import { act, fireEvent, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { Launcher } from "@/components/Launcher";
import {
  LAUNCHER_INTERNAL_TOOLS_ORDER_KEY,
  writeSavedOrder,
} from "@/lib/launcherToolOrder";
import { setLauncherToolVisible } from "@/lib/launcherToolVisibility";
import { renderWithProviders } from "@/test/mocks/render";
import { resetTauriMocks, invokeMock } from "@/test/mocks/tauri";
import {
  resetMessageMocks,
  safeRecordMessageMock,
} from "@/test/mocks/messages";

const launcherItems = [
  {
    id: "script-1",
    name: "Danger Script",
    type: "script",
    target: "rm -rf ./tmp",
    pinned: false,
    pin_order: 0,
    launch_count: 0,
    trusted: false,
    created_at: 1,
    updated_at: 1,
  },
];

describe("Launcher", () => {
  beforeEach(() => {
    localStorage.clear();
    resetTauriMocks();
    resetMessageMocks();
    invokeMock.mockImplementation(async (command: string, args?: unknown) => {
      if (command === "launcher_list") {
        return { data: launcherItems };
      }
      if (command === "ssh_tunnels_snapshot") {
        return { groups: [], tunnels: [], runtime: [] };
      }
      if (command === "launcher_mark_launched") {
        return null;
      }
      if (command === "launcher_execute") {
        return null;
      }
      if (command === "launcher_set_trust") {
        return null;
      }
      if (command === "dashboard_counts") {
        return { data: {} };
      }
      if (command === "get_storage_config") {
        return {};
      }
      if (command === "protocol_router_status") {
        return { running: false, enabled: false, port: 0, route_count: 0 };
      }
      throw new Error(`Unhandled command: ${command} ${JSON.stringify(args)}`);
    });
  });

  it("keeps script trust confirmation modal and confirms successful trusted run", async () => {
    const user = userEvent.setup();
    renderWithProviders(<Launcher />);

    expect(await screen.findByText("Danger Script")).toBeInTheDocument();
    fireEvent.click(screen.getByText("Danger Script"));

    expect(
      await screen.findByText(/Run untrusted command\?|是否执行未信任命令？/),
    ).toBeInTheDocument();
    await user.click(
      screen.getByLabelText(
        /Trust this launcher item for future runs|信任此启动项，后续直接执行/,
      ),
    );
    await user.click(screen.getByRole("button", { name: /Launch|启动/ }));

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("launcher_set_trust", {
        payload: { itemId: "script-1", trusted: true },
      });
      expect(invokeMock).toHaveBeenCalledWith("launcher_execute", {
        payload: { type: "script", target: "rm -rf ./tmp" },
      });
    });
    expect(safeRecordMessageMock).toHaveBeenCalledWith(
      expect.objectContaining({
        severity: "success",
        dedupe_key: "launcher:execute:success:script-1",
      }),
    );
  });

  it("cancels untrusted script launch without executing", async () => {
    const user = userEvent.setup();
    renderWithProviders(<Launcher />);

    fireEvent.click(await screen.findByText("Danger Script"));
    await user.click(screen.getByRole("button", { name: /Cancel|取消/ }));

    await waitFor(() => {
      expect(
        screen.queryByText(/Run untrusted command\?|是否执行未信任命令？/),
      ).not.toBeInTheDocument();
    });
    expect(invokeMock).not.toHaveBeenCalledWith(
      "launcher_execute",
      expect.anything(),
    );
  });

  it("records failure when untrusted script execution fails", async () => {
    const user = userEvent.setup();
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "launcher_list") return { data: launcherItems };
      if (command === "ssh_tunnels_snapshot") return { groups: [], tunnels: [], runtime: [] };
      if (command === "protocol_router_status") {
        return { running: false, enabled: false, port: 0, route_count: 0 };
      }
      if (command === "get_storage_config") return {};
      if (command === "launcher_execute") throw new Error("permission denied");
      if (command === "launcher_mark_launched") return null;
      throw new Error(`Unhandled command: ${command}`);
    });

    renderWithProviders(<Launcher />);
    fireEvent.click(await screen.findByText("Danger Script"));
    await user.click(screen.getByRole("button", { name: /Launch|启动/ }));

    await waitFor(() => {
      expect(safeRecordMessageMock).toHaveBeenCalledWith(
        expect.objectContaining({
          severity: "error",
          dedupe_key: "launcher:execute:error:script-1",
        }),
      );
    });
  });

  it("展示全部更多工具并导航到各自详情", async () => {
    const user = userEvent.setup();
    const setActiveTab = vi.fn();
    (
      window as typeof window & {
        setActiveTab?: (target: string) => void;
      }
    ).setActiveTab = setActiveTab;
    renderWithProviders(<Launcher />);

    const tools = [
      [/Bookmarks|书签|收藏夹/, "bookmarks"],
      [/Cloud Drive|云盘/, "cloud"],
      [/SSH Servers|SSH 服务器/, "ssh"],
      [/SSH Tunnels|SSH 隧道/, "ssh-tunnels"],
      [/Protocol Router|协议路由/, "protocol-router"],
      [/Random Password|随机密码/, "random-password"],
      [/JSON Parser|JSON 解析/, "json-parser"],
      [/MD5 Encryption|MD5 加密/, "md5-encryption"],
      [/Short Link|生成短链接/, "short-link"],
      [/File Sharing|文件共享/, "file-sharing"],
    ] as const;

    for (const [name, target] of tools) {
      const tool = await screen.findByRole("button", { name });
      await user.click(tool);
      expect(setActiveTab).toHaveBeenLastCalledWith(target);
    }
  });

  it.each([
    ["bookmarks", "lucide-star", "bg-amber-500/10 text-amber-600"],
    ["cloud", "lucide-cloud", "bg-sky-500/10 text-sky-600"],
    ["ssh", "lucide-server", "bg-blue-500/10 text-blue-600"],
    ["ssh-tunnels", "lucide-waypoints", "bg-cyan-500/10 text-cyan-600"],
    ["protocol-router", "lucide-route", "bg-orange-500/10 text-orange-600"],
    ["random-password", "lucide-key-round", "bg-emerald-500/10 text-emerald-600"],
    ["json-parser", "lucide-braces", "bg-sky-500/10 text-sky-600"],
    ["md5-encryption", "lucide-hash", "bg-teal-500/10 text-teal-600"],
    ["short-link", "lucide-link", "bg-teal-500/10 text-teal-600"],
    ["file-sharing", "lucide-share-2", "bg-rose-500/10 text-rose-600"],
  ] as const)(
    "为 %s 复用更多工具的图标展示",
    async (toolId, iconClassName, iconContainerClassName) => {
      renderWithProviders(<Launcher />);

      const iconContainer = await screen.findByTestId(
        `launcher-tool-icon-${toolId}`,
      );

      expect(iconContainer).toHaveClass(...iconContainerClassName.split(" "));
      expect(iconContainer.querySelector("svg")).toHaveClass(iconClassName);
    },
  );

  it("可按短链接中英文名称和简介搜索并打开稳定入口", async () => {
    const user = userEvent.setup();
    const setActiveTab = vi.fn();
    (
      window as typeof window & {
        setActiveTab?: (target: string) => void;
      }
    ).setActiveTab = setActiveTab;
    renderWithProviders(<Launcher />);

    const search = screen.getByPlaceholderText(
      /Search launcher items|搜索启动项/,
    );
    for (const term of [
      "Short Link",
      "Create a TinyURL short link",
      "生成短链接",
      "使用 TinyURL 生成短链接",
    ]) {
      await user.clear(search);
      await user.type(search, term);
      expect(
        await screen.findByTestId("launcher-tool-icon-short-link"),
      ).toBeInTheDocument();
    }

    await user.click(
      screen.getByTestId("launcher-tool-icon-short-link").closest("button")!,
    );
    expect(setActiveTab).toHaveBeenLastCalledWith("short-link");
  });

  it("显式隐藏后不展示短链接工具或搜索结果", async () => {
    const user = userEvent.setup();
    setLauncherToolVisible("short-link", false);
    renderWithProviders(<Launcher />);

    expect(await screen.findByText("Danger Script")).toBeInTheDocument();
    expect(
      screen.queryByTestId("launcher-tool-icon-short-link"),
    ).not.toBeInTheDocument();

    await user.type(
      screen.getByPlaceholderText(/Search launcher items|搜索启动项/),
      "生成短链接",
    );
    expect(
      screen.queryByTestId("launcher-tool-icon-short-link"),
    ).not.toBeInTheDocument();
  });

  it("根据持久化可见性过滤更多工具", async () => {
    renderWithProviders(<Launcher />);

    act(() => {
      for (const tool of [
        "bookmarks",
        "cloud",
        "ssh",
        "ssh-tunnels",
        "protocol-router",
        "random-password",
        "json-parser",
        "md5Encryption",
        "short-link",
        "file-sharing",
      ] as const) {
        setLauncherToolVisible(tool, false);
      }
    });

    await waitFor(() => {
      expect(screen.queryByText("收藏夹")).not.toBeInTheDocument();
      expect(screen.queryByText("Cloud Drive")).not.toBeInTheDocument();
      expect(screen.queryByText("SSH Servers")).not.toBeInTheDocument();
      expect(screen.queryByText("SSH Tunnels")).not.toBeInTheDocument();
      expect(screen.queryByText("Protocol Router")).not.toBeInTheDocument();
      expect(screen.queryByText("随机密码")).not.toBeInTheDocument();
      expect(screen.queryByText("JSON 解析")).not.toBeInTheDocument();
      expect(
        screen.queryByText(/MD5 Encryption|MD5 加密/),
      ).not.toBeInTheDocument();
      expect(
        screen.queryByTestId("launcher-tool-icon-short-link"),
      ).not.toBeInTheDocument();
    });
  });

  it("展示 JT/T 数据解析总入口并导航到稳定工具 ID", async () => {
    const user = userEvent.setup();
    const setActiveTab = vi.fn();
    (
      window as typeof window & {
        setActiveTab?: (target: string) => void;
      }
    ).setActiveTab = setActiveTab;
    renderWithProviders(<Launcher />);

    const total = await screen.findByTestId("launcher-tool-icon-jtt-data-parser");
    await user.click(total.closest("button")!);
    expect(setActiveTab).toHaveBeenLastCalledWith("jtt-data-parser");
  });

  it("搜索 808/809/1078/Hex 时展示匹配别名并打开对应子标签页", async () => {
    const user = userEvent.setup();
    const setActiveTab = vi.fn();
    (
      window as typeof window & {
        setActiveTab?: (target: string) => void;
      }
    ).setActiveTab = setActiveTab;
    renderWithProviders(<Launcher />);

    await user.type(
      screen.getByPlaceholderText(/Search launcher items|搜索启动项/),
      "808",
    );
    const alias808 = await screen.findByTestId("launcher-tool-icon-808");
    await user.click(alias808.closest("button")!);
    expect(setActiveTab).toHaveBeenLastCalledWith("808");

    for (const [term, target, testId] of [
      ["809", "809", "launcher-tool-icon-809"],
      ["1078", "1078", "launcher-tool-icon-1078"],
      ["Hex", "hex", "launcher-tool-icon-hex"],
    ] as const) {
      await user.clear(screen.getByPlaceholderText(/Search launcher items|搜索启动项/));
      await user.type(screen.getByPlaceholderText(/Search launcher items|搜索启动项/), term);
      const alias = await screen.findByTestId(testId);
      await user.click(alias.closest("button")!);
      expect(setActiveTab).toHaveBeenLastCalledWith(target);
    }
  });

  it("别名仅在实际搜索时展示", async () => {
    renderWithProviders(<Launcher />);

    expect(await screen.findByText("Danger Script")).toBeInTheDocument();
    expect(screen.queryByTestId("launcher-tool-icon-808")).not.toBeInTheDocument();
    expect(screen.queryByTestId("launcher-tool-icon-hex")).not.toBeInTheDocument();
  });

  it("隐藏启动台展示后不再展示 JT/T 总入口或搜索别名", async () => {
    const user = userEvent.setup();
    setLauncherToolVisible("jtt-data-parser", false);
    renderWithProviders(<Launcher />);

    expect(await screen.findByText("Danger Script")).toBeInTheDocument();
    expect(
      screen.queryByTestId("launcher-tool-icon-jtt-data-parser"),
    ).not.toBeInTheDocument();

    await user.type(
      screen.getByPlaceholderText(/Search launcher items|搜索启动项/),
      "808",
    );
    expect(
      screen.queryByTestId("launcher-tool-icon-808"),
    ).not.toBeInTheDocument();
  });

  it("不展示已移除的内部启动项", async () => {
    invokeMock.mockImplementation(async (command: string, args?: unknown) => {
      if (command === "launcher_list") {
        return {
          data: [
            {
              id: "legacy-ai-flow",
              name: "Legacy AI Flow",
              type: "internal",
              target: "ai-flow",
              pinned: false,
              pin_order: 0,
              launch_count: 0,
              trusted: true,
              created_at: 1,
              updated_at: 1,
            },
            {
              id: "current-launcher",
              name: "Current Launcher",
              type: "internal",
              target: "launcher",
              pinned: false,
              pin_order: 0,
              launch_count: 0,
              trusted: true,
              created_at: 1,
              updated_at: 1,
            },
          ],
        };
      }
      if (command === "ssh_tunnels_snapshot") {
        return { groups: [], tunnels: [], runtime: [] };
      }
      if (command === "get_storage_config") return {};
      if (command === "protocol_router_status") {
        return { running: false, enabled: false, port: 0, route_count: 0 };
      }
      throw new Error(`Unhandled command: ${command} ${JSON.stringify(args)}`);
    });

    renderWithProviders(<Launcher />);

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("launcher_list");
    });
    expect(screen.queryByText("Legacy AI Flow")).not.toBeInTheDocument();
    expect(screen.getByText("Current Launcher")).toBeInTheDocument();
  });

  describe("内部工具卡片拖拽整理", () => {
    const internalCardOrder = () =>
      screen
        .getAllByTestId(/launcher-internal-tool-card-/)
        .map((card) =>
          card
            .getAttribute("data-testid")!
            .replace("launcher-internal-tool-card-", ""),
        );

    it("长按内部工具卡片拖拽把手进入拖拽模式并提示可拖拽", async () => {
      renderWithProviders(<Launcher />);
      const handle = await screen.findByTestId(
        "launcher-tool-drag-handle-quick-bookmarks",
      );

      vi.useFakeTimers();
      fireEvent.pointerDown(handle, { pointerId: 1, clientX: 20, clientY: 20 });
      act(() => vi.advanceTimersByTime(300));

      expect(
        screen.getByText(/可拖拽调整顺序|Drag cards to reorder/),
      ).toBeInTheDocument();
      expect(
        screen.getByRole("button", { name: /完成|Done/ }),
      ).toBeInTheDocument();

      fireEvent.pointerUp(window, { pointerId: 1 });
      vi.useRealTimers();
    });

    it("拖拽内部工具卡片划过目标卡片实时移动位置并持久化", async () => {
      renderWithProviders(<Launcher />);
      const handle = await screen.findByTestId(
        "launcher-tool-drag-handle-quick-bookmarks",
      );

      vi.useFakeTimers();
      fireEvent.pointerDown(handle, { pointerId: 1, clientX: 20, clientY: 20 });
      act(() => vi.advanceTimersByTime(300));
      fireEvent.pointerOver(
        screen.getByTestId("launcher-internal-tool-card-quick-ssh"),
        { pointerId: 1 },
      );

      expect(internalCardOrder().slice(0, 3)).toEqual([
        "quick-cloud",
        "quick-ssh",
        "quick-bookmarks",
      ]);
      expect(
        JSON.parse(
          localStorage.getItem(LAUNCHER_INTERNAL_TOOLS_ORDER_KEY) || "[]",
        ),
      ).toEqual([
        "quick-cloud",
        "quick-ssh",
        "quick-bookmarks",
        "quick-ssh-tunnels",
        "quick-protocol-router",
        "quick-random-password",
        "quick-json-parser",
        "quick-md5-encryption",
        "quick-short-link",
        "quick-file-sharing",
        "quick-jtt-data-parser",
      ]);

      fireEvent.pointerUp(window, { pointerId: 1 });
      vi.useRealTimers();
    });

    it("渲染时应用已保存的内部工具顺序", async () => {
      writeSavedOrder(LAUNCHER_INTERNAL_TOOLS_ORDER_KEY, [
        "quick-ssh",
        "quick-bookmarks",
        "quick-cloud",
      ]);
      renderWithProviders(<Launcher />);

      await screen.findByTestId("launcher-internal-tool-card-quick-bookmarks");
      expect(internalCardOrder().slice(0, 3)).toEqual([
        "quick-ssh",
        "quick-bookmarks",
        "quick-cloud",
      ]);
    });

    it("拖拽模式下点击卡片不打开工具，点击完成退出后恢复单击打开", async () => {
      const setActiveTab = vi.fn();
      (
        window as typeof window & {
          setActiveTab?: (target: string) => void;
        }
      ).setActiveTab = setActiveTab;
      renderWithProviders(<Launcher />);
      const handle = await screen.findByTestId(
        "launcher-tool-drag-handle-quick-bookmarks",
      );

      vi.useFakeTimers();
      fireEvent.pointerDown(handle, { pointerId: 1, clientX: 20, clientY: 20 });
      act(() => vi.advanceTimersByTime(300));
      fireEvent.click(
        screen.getByTestId("launcher-internal-tool-card-quick-ssh"),
      );
      expect(setActiveTab).not.toHaveBeenCalled();

      fireEvent.click(screen.getByRole("button", { name: /完成|Done/ }));
      expect(
        screen.queryByText(/可拖拽调整顺序|Drag cards to reorder/),
      ).not.toBeInTheDocument();

      fireEvent.click(
        screen.getByTestId("launcher-internal-tool-card-quick-ssh"),
      );
      expect(setActiveTab).toHaveBeenCalledWith("ssh");
      vi.useRealTimers();
    });
  });
});
