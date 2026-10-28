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
      if (command === "ssh_hosts_list") {
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
});
