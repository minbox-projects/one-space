import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { QuickSftpTool } from "./QuickSftpTool";
import { renderWithProviders } from "@/test/mocks/render";

describe("QuickSftpTool", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    vi.useRealTimers();
  });

  it("renders initially in disconnected state without auto-connecting", () => {
    renderWithProviders(<QuickSftpTool />);

    // 工具标题
    expect(
      screen.getByRole("heading", { name: /SFTP|SFTP 传输/i }),
    ).toBeInTheDocument();

    // 默认未连接，展示连接按钮而不是断开按钮
    expect(
      screen.getByRole("button", { name: /连接 \/ 登录|Connect \/ Login/i }),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: /断开连接|Disconnect/i }),
    ).not.toBeInTheDocument();

    // 未连接提示引导
    expect(
      screen.getByText(/尚未连接远程服务器|Not connected to remote host/i),
    ).toBeInTheDocument();

    // 路径输入框默认值
    expect(screen.getByPlaceholderText(/~ or \/var\/log/i)).toHaveValue("~");
  });

  it("updates path input when clicking bookmark in disconnected state", async () => {
    const user = userEvent.setup();
    renderWithProviders(<QuickSftpTool />);

    const varLogBtn = screen.getByRole("button", { name: "/var/log" });
    await user.click(varLogBtn);

    const pathInput = screen.getByPlaceholderText(/~ or \/var\/log/i);
    expect(pathInput).toHaveValue("/var/log");
  });

  it("connects upon clicking connect button, displays file list, and disconnects", async () => {
    const user = userEvent.setup();
    renderWithProviders(<QuickSftpTool />);

    // 填入自定义主机（如果没有预设 SSH 主机列表）
    const hostInput = screen.queryByPlaceholderText(/user@host or config alias/i);
    if (hostInput) {
      await user.type(hostInput, "test-user@192.168.1.100");
    }

    // 点击顶部的连接按钮
    const connectBtn = screen.getByRole("button", {
      name: /连接 \/ 登录|Connect \/ Login/i,
    });
    await user.click(connectBtn);

    // 连接成功后显示断开按钮与已连接徽章
    const disconnectBtn = await screen.findByRole("button", {
      name: /断开连接|Disconnect/i,
    });
    expect(disconnectBtn).toBeInTheDocument();
    expect(screen.getAllByText(/已连接|Connected/i).length).toBeGreaterThanOrEqual(1);

    // 展示远程文件
    expect(await screen.findByText("nginx")).toBeInTheDocument();
    expect(screen.getByText("app.log")).toBeInTheDocument();

    // 点击断开连接
    await user.click(disconnectBtn);

    // 恢复未连接状态
    expect(
      await screen.findByRole("button", {
        name: /连接 \/ 登录|Connect \/ Login/i,
      }),
    ).toBeInTheDocument();
    expect(screen.queryByText("nginx")).not.toBeInTheDocument();
    expect(
      screen.getByText(/尚未连接远程服务器|Not connected to remote host/i),
    ).toBeInTheDocument();
  });

  it("handles downloading a remote file and shows transfer progress card", async () => {
    const user = userEvent.setup();
    renderWithProviders(<QuickSftpTool />);

    // 连接
    const hostInput = screen.queryByPlaceholderText(/user@host or config alias/i);
    if (hostInput) {
      await user.type(hostInput, "test-user@192.168.1.100");
    }
    const connectBtn = screen.getByRole("button", {
      name: /连接 \/ 登录|Connect \/ Login/i,
    });
    await user.click(connectBtn);

    // 等待文件加载
    expect(await screen.findByText("app.log")).toBeInTheDocument();

    // 找到 app.log 所在行的下载按钮
    const downloadBtns = screen.getAllByTitle(/下载|Download/i);
    expect(downloadBtns.length).toBeGreaterThan(0);

    // 点击下载
    await user.click(downloadBtns[0]);

    // 应该展示传输任务面板并渲染文件名与下载标识
    expect(await screen.findByText(/传输任务|Transfers/i)).toBeInTheDocument();
    expect(screen.getAllByText("app.log").length).toBeGreaterThanOrEqual(1);
  });

  it("disables upload button in disconnected state and enables it after connecting", async () => {
    const user = userEvent.setup();
    renderWithProviders(<QuickSftpTool />);

    // 未连接状态下，上传按钮为禁用状态
    const uploadBtn = screen.getByRole("button", { name: /上传|Upload/i });
    expect(uploadBtn).toBeDisabled();

    // 连接
    const hostInput = screen.queryByPlaceholderText(/user@host or config alias/i);
    if (hostInput) {
      await user.type(hostInput, "test-user@192.168.1.100");
    }
    const connectBtn = screen.getByRole("button", {
      name: /连接 \/ 登录|Connect \/ Login/i,
    });
    await user.click(connectBtn);

    // 已连接后，上传按钮变为可用
    await screen.findByRole("button", {
      name: /断开连接|Disconnect/i,
    });
    expect(uploadBtn).not.toBeDisabled();
  });

  it("shows retry button on failed transfer and triggers retry upon clicking", async () => {
    // 模拟获取下载目录抛错触发失败状态
    const sftpModule = await import("@/lib/sftp");
    const spy = vi
      .spyOn(sftpModule, "sftpGetDefaultDownloadDir")
      .mockRejectedValueOnce(new Error("Permission denied to target directory"));

    const user = userEvent.setup();
    renderWithProviders(<QuickSftpTool />);

    // 连接
    const hostInput = screen.queryByPlaceholderText(/user@host or config alias/i);
    if (hostInput) {
      await user.type(hostInput, "test-user@192.168.1.100");
    }
    const connectBtn = screen.getByRole("button", {
      name: /连接 \/ 登录|Connect \/ Login/i,
    });
    await user.click(connectBtn);

    expect(await screen.findByText("app.log")).toBeInTheDocument();

    const downloadBtns = screen.getAllByTitle(/下载|Download/i);
    await user.click(downloadBtns[0]);

    // 应该展示传输面板并显示失败与重试按钮
    expect(await screen.findByText(/传输任务|Transfers/i)).toBeInTheDocument();
    expect(await screen.findByText(/传输失败|Failed/i)).toBeInTheDocument();

    const retryBtn = await screen.findByTitle(/重试|Retry/i);
    expect(retryBtn).toBeInTheDocument();

    // 点击重试按钮
    await user.click(retryBtn);

    spy.mockRestore();
  });

  it("prompts confirmation before deletion and does nothing if cancelled", async () => {
    const user = userEvent.setup();
    renderWithProviders(<QuickSftpTool />);

    // 连接
    const hostInput = screen.queryByPlaceholderText(/user@host or config alias/i);
    if (hostInput) {
      await user.type(hostInput, "test-user@192.168.1.100");
    }
    const connectBtn = screen.getByRole("button", {
      name: /连接 \/ 登录|Connect \/ Login/i,
    });
    await user.click(connectBtn);

    expect(await screen.findByText("app.log")).toBeInTheDocument();

    const appLogDeleteBtn = screen.getByRole("button", {
      name: /删除 app\.log|Delete app\.log/i,
    });
    // 点击删除
    await user.click(appLogDeleteBtn);

    // 弹出确认弹窗
    expect(
      await screen.findByText(/确定要将.*移入回收站吗|Are you sure you want to move/i),
    ).toBeInTheDocument();

    // 点击取消
    const cancelBtn = screen.getByRole("button", { name: /取消|Cancel/i });
    await user.click(cancelBtn);

    // 文件仍然存在
    expect(screen.getByText("app.log")).toBeInTheDocument();
  });

  it("deletes file with optimistic update, calls sftpDeleteEntry, and supports undo", async () => {
    const sftpModule = await import("@/lib/sftp");
    const deleteSpy = vi.spyOn(sftpModule, "sftpDeleteEntry").mockResolvedValueOnce({
      original_path: "~/app.log",
      trash_path: "~/.onespace_trash/123_app.log",
      is_permanent: false,
    });
    const restoreSpy = vi
      .spyOn(sftpModule, "sftpRestoreEntry")
      .mockResolvedValueOnce(undefined);

    const user = userEvent.setup();
    renderWithProviders(<QuickSftpTool />);

    // 连接
    const hostInput = screen.queryByPlaceholderText(/user@host or config alias/i);
    if (hostInput) {
      await user.type(hostInput, "test-user@192.168.1.100");
    }
    const connectBtn = screen.getByRole("button", {
      name: /连接 \/ 登录|Connect \/ Login/i,
    });
    await user.click(connectBtn);

    expect(await screen.findByText("app.log")).toBeInTheDocument();

    // 找到 app.log 行的专属删除按钮
    const appLogDeleteBtn = screen.getByRole("button", {
      name: /删除 app\.log|Delete app\.log/i,
    });
    await user.click(appLogDeleteBtn);

    // 确认弹窗
    expect(
      await screen.findByText(/确定要将.*移入回收站吗|Are you sure you want to move/i),
    ).toBeInTheDocument();

    // 点击弹窗中的确认删除按钮
    const confirmDeleteBtn = screen.getByRole("button", { name: /^删除$|^Delete$/i });
    await user.click(confirmDeleteBtn);

    // 乐观更新：已从表格中即刻移除
    await waitFor(() => {
      expect(screen.queryByText("app.log")).not.toBeInTheDocument();
    });

    // Toast 中出现【撤销】按钮
    const undoBtn = await screen.findByRole("button", { name: /撤销|Undo/i });
    expect(undoBtn).toBeInTheDocument();

    // 点击【撤销】
    await user.click(undoBtn);
    expect(restoreSpy).toHaveBeenCalledWith(
      expect.any(String),
      "~/.onespace_trash/123_app.log",
      "~/app.log",
    );

    deleteSpy.mockRestore();
    restoreSpy.mockRestore();
  });

  it("avoids full table loading flicker during silent refresh when files already exist", async () => {
    const user = userEvent.setup();
    renderWithProviders(<QuickSftpTool />);

    // 连接
    const hostInput = screen.queryByPlaceholderText(/user@host or config alias/i);
    if (hostInput) {
      await user.type(hostInput, "test-user@192.168.1.100");
    }
    const connectBtn = screen.getByRole("button", {
      name: /连接 \/ 登录|Connect \/ Login/i,
    });
    await user.click(connectBtn);

    expect(await screen.findByText("app.log")).toBeInTheDocument();

    // 点击刷新按钮
    const refreshBtn = screen.getByRole("button", { name: /刷新|Refresh/i });
    await user.click(refreshBtn);

    // 静默刷新期间，表格依然保留，绝不展示大占位 Loader 或白屏文案
    expect(
      screen.queryByText(/正在读取远程服务器目录|Reading remote directory/i),
    ).not.toBeInTheDocument();
    expect(screen.getByText("app.log")).toBeInTheDocument();
  });

  it("opens trash modal, shows items, restores an item, and cleans up after emptying", async () => {
    const sftpModule = await import("@/lib/sftp");
    const listSpy = vi.spyOn(sftpModule, "sftpListTrash").mockResolvedValue([
      {
        id: "1729584000_pelican.html",
        name: "pelican.html",
        original_path: "/root/pelican.html",
        trash_path: "~/.onespace_trash/1729584000_pelican.html",
        size: 2048,
        is_dir: false,
        deleted_at: "2026-10-21 16:00",
      },
    ]);
    const restoreSpy = vi
      .spyOn(sftpModule, "sftpRestoreTrashItem")
      .mockResolvedValue("/root/pelican.html");
    const clearSpy = vi.spyOn(sftpModule, "sftpClearTrash").mockResolvedValue(undefined);

    const user = userEvent.setup();
    renderWithProviders(<QuickSftpTool />);

    // 初始断开时，回收站按钮禁用
    const trashBtn = screen.getByRole("button", { name: /^回收站$|^Trash$/i });
    expect(trashBtn).toBeDisabled();

    // 连接
    const hostInput = screen.queryByPlaceholderText(/user@host or config alias/i);
    if (hostInput) {
      await user.type(hostInput, "test-user@192.168.1.100");
    }
    const connectBtn = screen.getByRole("button", {
      name: /连接 \/ 登录|Connect \/ Login/i,
    });
    await user.click(connectBtn);

    expect(await screen.findByText("app.log")).toBeInTheDocument();
    expect(trashBtn).not.toBeDisabled();

    // 点击打开回收站弹窗
    await user.click(trashBtn);

    // 应该展示回收站弹窗标题和已回收的文件
    expect(await screen.findByText(/远程回收站|Remote Trash/i)).toBeInTheDocument();
    expect(await screen.findByText("pelican.html")).toBeInTheDocument();
    expect(screen.getByText("/root/pelican.html")).toBeInTheDocument();

    // 点击恢复按钮
    const restoreBtn = screen.getByRole("button", { name: /恢复|Restore/i });
    await user.click(restoreBtn);

    // 恢复函数被调用
    await waitFor(() => {
      expect(restoreSpy).toHaveBeenCalledWith(expect.any(String), "1729584000_pelican.html");
    });

    // 列表项被乐观移出，展示空状态
    expect(await screen.findByText(/回收站为空|Trash is empty/i)).toBeInTheDocument();

    listSpy.mockRestore();
    restoreSpy.mockRestore();
    clearSpy.mockRestore();
  });

  it("confirms before emptying remote trash and empties upon confirmation", async () => {
    const sftpModule = await import("@/lib/sftp");
    const listSpy = vi.spyOn(sftpModule, "sftpListTrash").mockResolvedValue([
      {
        id: "1729584000_temp.txt",
        name: "temp.txt",
        original_path: "/root/temp.txt",
        trash_path: "~/.onespace_trash/1729584000_temp.txt",
        size: 100,
        is_dir: false,
        deleted_at: "2026-10-21 16:00",
      },
    ]);
    const clearSpy = vi.spyOn(sftpModule, "sftpClearTrash").mockResolvedValue(undefined);

    const user = userEvent.setup();
    renderWithProviders(<QuickSftpTool />);

    // 连接
    const hostInput = screen.queryByPlaceholderText(/user@host or config alias/i);
    if (hostInput) {
      await user.type(hostInput, "test-user@192.168.1.100");
    }
    const connectBtn = screen.getByRole("button", {
      name: /连接 \/ 登录|Connect \/ Login/i,
    });
    await user.click(connectBtn);

    expect(await screen.findByText("app.log")).toBeInTheDocument();

    // 点击回收站
    const trashBtn = screen.getByRole("button", { name: /^回收站$|^Trash$/i });
    await user.click(trashBtn);

    expect(await screen.findByText("temp.txt")).toBeInTheDocument();

    // 点击清空回收站按钮
    const clearTrashBtn = screen.getByRole("button", { name: /清空回收站|Empty Trash/i });
    await user.click(clearTrashBtn);

    // 确认弹窗
    expect(
      await screen.findByText(/确定要永久彻底清空回收站|Are you sure you want to permanently delete all items in trash/i),
    ).toBeInTheDocument();

    // 确认弹窗中的确认按钮（取最上层弹窗内的确认按钮）
    const confirmButtons = screen.getAllByRole("button", {
      name: /^清空回收站$|^Empty Trash$/i,
    });
    await user.click(confirmButtons[confirmButtons.length - 1]);

    await waitFor(() => {
      expect(clearSpy).toHaveBeenCalled();
    });

    // 回收站变为空
    expect(await screen.findByText(/回收站为空|Trash is empty/i)).toBeInTheDocument();

    listSpy.mockRestore();
    clearSpy.mockRestore();
  });
});
