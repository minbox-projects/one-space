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
});
