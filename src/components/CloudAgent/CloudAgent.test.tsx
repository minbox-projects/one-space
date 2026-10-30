import { act, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { CloudAgent } from "./index";
import { renderWithProviders } from "@/test/mocks/render";
import { invokeMock, resetTauriMocks } from "@/test/mocks/tauri";

const { pushToastMock } = vi.hoisted(() => ({ pushToastMock: vi.fn() }));

vi.mock("@/components/ToastProvider", async () => {
  const actual = await vi.importActual<
    typeof import("@/components/ToastProvider")
  >("@/components/ToastProvider");
  return {
    ...actual,
    useToast: () => ({ pushToast: pushToastMock, dismissToast: vi.fn() }),
  };
});

describe("CloudAgent Component", () => {
  beforeEach(() => {
    resetTauriMocks();
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "cloud_agent_get_settings") {
        return {
          default_host: "dmit-server",
          remote_workspace_root: "/root/cloud-agent-workspaces",
          remote_agent_dir: "/root/onespace-cloud-agent",
          opencode_yolo_mode: true,
          opencode_extra_args: "",
          auto_refresh_interval_seconds: 10,
          disk_alert_threshold_percent: 80,
          terminal_app: "auto",
        };
      }
      if (command === "get_ssh_hosts" || command === "ssh_hosts_list") {
        return [{ name: "dmit-server", host_name: "1.2.3.4" }];
      }
      if (command === "cloud_agent_get_status") {
        return {
          service_active: true,
          disk: {
            total_gb: 50.0,
            used_gb: 24.5,
            free_gb: 25.5,
            use_percent: 49.0,
          },
          gateway: {
            status: "healthy",
            port: 17688,
            providers_count: 3,
            model_pools: ["coder-fast"],
          },
          timestamp: 1700000000,
        };
      }
      if (command === "cloud_agent_list_sessions") {
        return [
          {
            session_name: "feat-auth-1234",
            repo_name: "one-space",
            branch: "feat-auth",
            working_dir: "/root/cloud-agent-workspaces/one-space/worktrees/feat-auth",
            created_at: 1700000000,
            windows_count: 1,
            pane_pid: 12345,
            is_active: true,
            opencode_session_id: "ses_opencode_999",
          },
        ];
      }
      if (command === "cloud_agent_list_worktrees") {
        return [
          {
            repo_name: "one-space",
            branch: "feat-auth",
            worktree_path: "/root/cloud-agent-workspaces/one-space/worktrees/feat-auth",
            has_active_session: true,
            session_name: "feat-auth-1234",
            last_commit: {
              hash: "abc1234",
              author: "Developer",
              timestamp: 1700000000,
              message: "feat: add user login",
            },
            last_modified: 1700000000,
          },
        ];
      }
      if (command === "cloud_agent_list_tasks") {
        return [
          {
            id: "task-1",
            repo: "one-space",
            issue_number: 42,
            title: "Implement login api",
            branch: "feat-issue-42",
            status: "completed",
            pr_url: "https://github.com/example/one-space/pull/1",
            started_at: 1700000000,
            finished_at: 1700001000,
            duration_seconds: 1000,
            error_message: null,
          },
        ];
      }
      if (command === "cloud_agent_launch_terminal") {
        return { success: true, message: "Terminal launched" };
      }
      if (command === "cloud_agent_sync_gateway") {
        return {
          success: true,
          providers_synced: 3,
          model_pools_synced: 10,
          message: "Synced successfully",
        };
      }
      return null;
    });
  });

  it("renders header and active sessions properly", async () => {
    await act(async () => {
      renderWithProviders(<CloudAgent isVisible={true} />);
    });

    await waitFor(() => {
      expect(screen.getAllByText("feat-auth").length).toBeGreaterThan(0);
    });

    // Verify session badge and repo name
    expect(screen.getByText("one-space")).toBeTruthy();
    await waitFor(() => {
      expect(screen.getByText("守护进程运行中")).toBeTruthy();
      expect(screen.getByText(/24.5G \/ 50G/)).toBeTruthy();
    });
  });

  it("handles terminal launch trigger", async () => {
    const user = userEvent.setup();

    await act(async () => {
      renderWithProviders(<CloudAgent isVisible={true} />);
    });

    await waitFor(() => {
      expect(screen.getAllByText("feat-auth").length).toBeGreaterThan(0);
    });

    const connectBtn = screen.getByText(/恢复会话/);
    await user.click(connectBtn);

    expect(invokeMock).toHaveBeenCalledWith(
      "cloud_agent_launch_terminal",
      expect.objectContaining({
        target: "feat-auth-1234",
      })
    );
  });

  it("switches tabs between sessions, worktrees, and pipeline", async () => {
    const user = userEvent.setup();

    await act(async () => {
      renderWithProviders(<CloudAgent isVisible={true} />);
    });

    await waitFor(() => {
      expect(screen.getAllByText("feat-auth").length).toBeGreaterThan(0);
    });

    // Switch to Worktrees Tab
    const worktreeTabBtn = screen.getByText("历史工作区");
    await user.click(worktreeTabBtn);

    await waitFor(() => {
      expect(screen.getByText("feat: add user login")).toBeTruthy();
    });

    // Switch to PR Pipeline Tab
    const pipelineTabBtn = screen.getByText("Issues 流水线");
    await user.click(pipelineTabBtn);

    await waitFor(() => {
      expect(screen.getByText("Implement login api")).toBeTruthy();
    });
  });

  it("navigates to cloud-agent settings on clicking settings icon", async () => {
    const user = userEvent.setup();
    const setSettingsTabMock = vi.fn();
    const setSettingsOpenMock = vi.fn();
    (window as any).setSettingsTab = setSettingsTabMock;
    (window as any).setSettingsOpen = setSettingsOpenMock;

    await act(async () => {
      renderWithProviders(<CloudAgent isVisible={true} />);
    });

    await waitFor(() => {
      expect(screen.getAllByText("feat-auth").length).toBeGreaterThan(0);
    });

    const settingsBtn = screen.getByTitle("设置");
    await user.click(settingsBtn);

    expect(setSettingsTabMock).toHaveBeenCalledWith("cloud-agent");
    expect(setSettingsOpenMock).toHaveBeenCalledWith(true);
  });

  it("renders metric cards and supports quick status filters in active sessions", async () => {
    const user = userEvent.setup();

    await act(async () => {
      renderWithProviders(<CloudAgent isVisible={true} />);
    });

    await waitFor(() => {
      expect(screen.getAllByText("feat-auth").length).toBeGreaterThan(0);
    });

    // 验证 4 组核心数据指标卡存在（注意活跃会话既在 Tab 也作为指标卡渲染）
    expect(screen.getAllByText("活跃会话").length).toBeGreaterThanOrEqual(2);
    expect(screen.getByText("终端作业窗口")).toBeTruthy();
    expect(screen.getByText("特性隔离空间")).toBeTruthy();
    expect(screen.getByText("涉及代码仓库")).toBeTruthy();

    // 验证状态快捷过滤药丸组存在
    expect(screen.getByText("全部会话")).toBeTruthy();
    expect(screen.getByText("活跃中")).toBeTruthy();
    expect(screen.getByText("已离线")).toBeTruthy();

    // 点击“已离线”过滤
    const idleFilterBtn = screen.getByText("已离线");
    await user.click(idleFilterBtn);

    // 切换回“全部会话”
    const allFilterBtn = screen.getByText("全部会话");
    await user.click(allFilterBtn);

    await waitFor(() => {
      expect(screen.getAllByText("feat-auth").length).toBeGreaterThan(0);
      expect(screen.getByText("ses_opencode_999")).toBeTruthy();
      expect(screen.getByText("feat-auth-1234")).toBeTruthy();
    });
  });

  it("does not render sync gateway button in header", async () => {
    await act(async () => {
      renderWithProviders(<CloudAgent isVisible={true} />);
    });

    await waitFor(() => {
      expect(screen.getAllByText("feat-auth").length).toBeGreaterThan(0);
    });

    // 验证顶部快捷操作栏中不存在“同步网关”按钮
    expect(screen.queryByText("同步网关")).toBeNull();
  });

  it("renders consistent empty state with action buttons when data is empty", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "cloud_agent_get_settings") {
        return {
          default_host: "dmit-server",
          remote_workspace_root: "/root/cloud-agent-workspaces",
          remote_agent_dir: "/root/onespace-cloud-agent",
        };
      }
      if (command === "get_ssh_hosts" || command === "ssh_hosts_list") {
        return [{ name: "dmit-server", host_name: "1.2.3.4" }];
      }
      if (command === "cloud_agent_get_status") {
        return { service_active: true };
      }
      if (command === "cloud_agent_list_sessions") return [];
      if (command === "cloud_agent_list_worktrees") return [];
      if (command === "cloud_agent_list_tasks") return [];
      return null;
    });

    const user = userEvent.setup();

    await act(async () => {
      renderWithProviders(<CloudAgent isVisible={true} />);
    });

    // 活跃会话空状态与统一按钮
    await waitFor(() => {
      expect(screen.getByText("当前没有正在运行的云端开发会话")).toBeInTheDocument();
      expect(screen.getByText("新建云端特性任务")).toBeInTheDocument();
    });

    // 切换到历史工作区 Tab 检查空状态与统一按钮
    const worktreesTab = screen.getByText("历史工作区");
    await user.click(worktreesTab);
    await waitFor(() => {
      expect(screen.getByText("暂无 Git Worktree 历史工作区")).toBeInTheDocument();
      expect(screen.getByText("新建云端特性任务")).toBeInTheDocument();
    });

    // 切换到 Issues 流水线 Tab 检查空状态与统一刷新按钮
    const pipelineTab = screen.getByText("Issues 流水线");
    await user.click(pipelineTab);
    await waitFor(() => {
      expect(screen.getByText("暂无 Issues 自动流水线任务记录")).toBeInTheDocument();
      expect(screen.getByText("刷新任务流水")).toBeInTheDocument();
    });
  });

  it("dispatches active sessions count event when sessions load", async () => {
    const dispatchSpy = vi.spyOn(window, "dispatchEvent");
    await act(async () => {
      renderWithProviders(<CloudAgent isVisible={true} />);
    });

    await waitFor(() => {
      expect(dispatchSpy).toHaveBeenCalledWith(
        expect.objectContaining({
          type: "onespace:cloud-agent-sessions-count",
          detail: { count: 1 },
        }),
      );
    });
    dispatchSpy.mockRestore();
  });

  it("renders empty state and guides to settings when no host is configured or available", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "cloud_agent_get_settings") {
        return {
          default_host: "",
          remote_workspace_root: "/root/cloud-agent-workspaces",
          remote_agent_dir: "/root/onespace-cloud-agent",
        };
      }
      if (command === "get_ssh_hosts" || command === "ssh_hosts_list") {
        return [];
      }
      return null;
    });

    await act(async () => {
      renderWithProviders(<CloudAgent isVisible={true} />);
    });

    await waitFor(() => {
      expect(screen.getByText("尚未选择或配置云端主机")).toBeInTheDocument();
      expect(screen.getAllByText("未配置云端主机").length).toBeGreaterThan(0);
      expect(screen.getByText("前往设置")).toBeInTheDocument();
    });
  });

  it("auto-selects first available ssh host when default_host is not configured", async () => {
    invokeMock.mockImplementation(async (command: string, args?: any) => {
      if (command === "cloud_agent_get_settings") {
        return {
          default_host: "",
          remote_workspace_root: "/root/cloud-agent-workspaces",
          remote_agent_dir: "/root/onespace-cloud-agent",
        };
      }
      if (command === "get_ssh_hosts" || command === "ssh_hosts_list") {
        return [{ name: "my-aliyun-vps", host_name: "47.1.2.3" }];
      }
      if (command === "cloud_agent_get_status") {
        return {
          service_active: true,
          disk: { total_gb: 40, used_gb: 10, free_gb: 30, use_percent: 25 },
          gateway: { status: "healthy", port: 17688 },
          timestamp: 1700000000,
        };
      }
      if (command === "cloud_agent_list_sessions") {
        if (args?.host === "my-aliyun-vps") {
          return [
            {
              session_name: "aliyun-ses-1",
              repo_name: "my-aliyun-repo",
              branch: "feat-cloud",
              working_dir: "/root/workspaces/feat",
              created_at: 1700000000,
              windows_count: 1,
              pane_pid: 9999,
              is_active: true,
            },
          ];
        }
        return [];
      }
      if (command === "cloud_agent_list_worktrees") return [];
      if (command === "cloud_agent_list_tasks") return [];
      return null;
    });

    await act(async () => {
      renderWithProviders(<CloudAgent isVisible={true} />);
    });

    await waitFor(() => {
      expect(screen.getByText("my-aliyun-repo")).toBeInTheDocument();
      expect(screen.getByText("aliyun-ses-1")).toBeInTheDocument();
      expect(screen.getByText("守护进程运行中")).toBeInTheDocument();
    });
  });

  it("clears old data immediately and loads new host data on settings updated event", async () => {
    let currentConfiguredDefaultHost = "host-alpha";

    invokeMock.mockImplementation(async (command: string, args?: any) => {
      if (command === "cloud_agent_get_settings") {
        return {
          default_host: currentConfiguredDefaultHost,
          remote_workspace_root: "/root/cloud-agent-workspaces",
          remote_agent_dir: "/root/onespace-cloud-agent",
        };
      }
      if (command === "get_ssh_hosts" || command === "ssh_hosts_list") {
        return [
          { name: "host-alpha", host_name: "1.1.1.1" },
          { name: "host-beta", host_name: "2.2.2.2" },
        ];
      }
      if (command === "cloud_agent_get_status") {
        return { service_active: true };
      }
      if (command === "cloud_agent_list_sessions") {
        if (args?.host === "host-alpha") {
          return [
            {
              session_name: "ses-alpha-only",
              repo_name: "repo-alpha",
              branch: "main",
              working_dir: "/root/alpha",
              created_at: 1700000000,
              windows_count: 1,
              pane_pid: 111,
              is_active: true,
            },
          ];
        }
        if (args?.host === "host-beta") {
          return [
            {
              session_name: "ses-beta-only",
              repo_name: "repo-beta",
              branch: "dev",
              working_dir: "/root/beta",
              created_at: 1700000000,
              windows_count: 1,
              pane_pid: 222,
              is_active: true,
            },
          ];
        }
        return [];
      }
      if (command === "cloud_agent_list_worktrees") return [];
      if (command === "cloud_agent_list_tasks") return [];
      return null;
    });

    await act(async () => {
      renderWithProviders(<CloudAgent isVisible={true} />);
    });

    await waitFor(() => {
      expect(screen.getByText("repo-alpha")).toBeInTheDocument();
      expect(screen.getByText("ses-alpha-only")).toBeInTheDocument();
    });

    // 模拟设置面板保存了 host-beta 并广播全局事件
    currentConfiguredDefaultHost = "host-beta";
    await act(async () => {
      window.dispatchEvent(
        new CustomEvent("onespace:cloud-agent-settings-updated", {
          detail: {
            defaultHost: "host-beta",
            remoteWorkspaceRoot: "/root/cloud-agent-workspaces",
            remoteAgentDir: "/root/onespace-cloud-agent",
            opencodeYoloMode: true,
            opencodeExtraArgs: "",
            autoRefreshIntervalSeconds: 10,
            diskAlertThresholdPercent: 80,
            terminalApp: "auto",
          },
        })
      );
    });

    await waitFor(() => {
      expect(screen.queryByText("repo-alpha")).toBeNull();
      expect(screen.queryByText("ses-alpha-only")).toBeNull();
      expect(screen.getByText("repo-beta")).toBeInTheDocument();
      expect(screen.getByText("ses-beta-only")).toBeInTheDocument();
    });
  });
});
