import { act, fireEvent, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { Launcher } from "@/components/Launcher";
import { setLauncherToolVisible } from "@/lib/launcherToolVisibility";
import { getMoreToolPresentation } from "@/lib/moreToolPresentation";
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
      [/File Sharing|文件共享/, "file-sharing"],
    ] as const;

    for (const [name, target] of tools) {
      const tool = await screen.findByRole("button", { name });
      await user.click(tool);
      expect(setActiveTab).toHaveBeenLastCalledWith(target);
    }
  });

  it.each([
    ["md5-encryption", "lucide-hash"],
    ["file-sharing", "lucide-share-2"],
  ] as const)("为 %s 复用更多工具的图标展示", async (toolId, iconClassName) => {
    renderWithProviders(<Launcher />);

    const iconContainer = await screen.findByTestId(
      `launcher-tool-icon-${toolId}`,
    );
    const presentation = getMoreToolPresentation(toolId);

    expect(iconContainer).toHaveClass(...presentation.iconClassName.split(" "));
    expect(iconContainer.querySelector("svg")).toHaveClass(iconClassName);
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
    });
  });

  it("不展示历史 ai-flow 内部启动项", async () => {
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
});
