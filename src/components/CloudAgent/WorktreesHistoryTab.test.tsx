import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { WorktreesHistoryTab } from "./components/WorktreesHistoryTab";
import type { CloudWorktree } from "@/lib/cloudAgent";

describe("WorktreesHistoryTab Component", () => {
  const dummyT = ((key: string, fallback?: string) => fallback ?? key) as any;

  const mockWorktrees: CloudWorktree[] = [
    {
      repo_name: "one-space",
      branch: "feat-auth",
      worktree_path: "/root/cloud-agent-workspaces/one-space-worktrees/feat-auth",
      has_active_session: true,
      session_name: "dev_one-space_feat-auth",
      last_commit: {
        hash: "a1b2c3d",
        author: "Alice",
        timestamp: 1700000000,
        message: "feat: add oauth2 login",
      },
      last_modified: 1700000000,
    },
    {
      repo_name: "one-space",
      branch: "fix-bug-123",
      worktree_path: "/root/cloud-agent-workspaces/one-space-worktrees/fix-bug-123",
      has_active_session: false,
      session_name: "dev_one-space_fix-bug-123",
      last_commit: {
        hash: "e4f5g6h",
        author: "Bob",
        timestamp: 1699990000,
        message: "fix: crash on start",
      },
      last_modified: 1699990000,
    },
    {
      repo_name: "backend-service",
      branch: "main",
      worktree_path: "/root/cloud-agent-workspaces/backend-service",
      has_active_session: false,
      session_name: "dev_backend-service",
      last_commit: null,
      last_modified: 1699980000,
    },
  ];

  it("renders statistics and worktree card details", () => {
    const onLaunchTerminal = vi.fn();
    const onOpenNewTaskModal = vi.fn();

    render(
      <WorktreesHistoryTab
        t={dummyT}
        language="zh"
        worktrees={mockWorktrees}
        loading={false}
        onLaunchTerminal={onLaunchTerminal}
        onOpenNewTaskModal={onOpenNewTaskModal}
      />
    );

    // Verify stats
    expect(screen.getByText("全部工作区")).toBeTruthy();
    expect(screen.getByText("3")).toBeTruthy(); // total
    expect(screen.getByText("1")).toBeTruthy(); // active
    expect(screen.getByText("2")).toBeTruthy(); // idle

    // Verify card contents
    expect(screen.getAllByText("feat-auth").length).toBeGreaterThan(0);
    expect(screen.getAllByText("fix-bug-123").length).toBeGreaterThan(0);
    expect(screen.getAllByText("backend-service").length).toBeGreaterThan(0);
    expect(screen.getByText("feat: add oauth2 login")).toBeTruthy();
    expect(screen.getByText("fix: crash on start")).toBeTruthy();
  });

  it("filters worktrees by search query", async () => {
    const user = userEvent.setup();
    const onLaunchTerminal = vi.fn();
    const onOpenNewTaskModal = vi.fn();

    render(
      <WorktreesHistoryTab
        t={dummyT}
        language="zh"
        worktrees={mockWorktrees}
        loading={false}
        onLaunchTerminal={onLaunchTerminal}
        onOpenNewTaskModal={onOpenNewTaskModal}
      />
    );

    const searchInput = screen.getByPlaceholderText("搜索分支 / 特性 / Commit...");
    await user.type(searchInput, "crash");

    expect(screen.queryByText("feat: add oauth2 login")).toBeNull();
    expect(screen.getByText("fix: crash on start")).toBeTruthy();
  });

  it("filters worktrees by status button", async () => {
    const user = userEvent.setup();
    const onLaunchTerminal = vi.fn();
    const onOpenNewTaskModal = vi.fn();

    render(
      <WorktreesHistoryTab
        t={dummyT}
        language="zh"
        worktrees={mockWorktrees}
        loading={false}
        onLaunchTerminal={onLaunchTerminal}
        onOpenNewTaskModal={onOpenNewTaskModal}
      />
    );

    const activeFilterBtn = screen.getByText("活跃运行");
    await user.click(activeFilterBtn);

    expect(screen.getAllByText("feat-auth").length).toBeGreaterThan(0);
    expect(screen.queryByText("fix-bug-123")).toBeNull();
  });

  it("triggers terminal launch on clicking resume / revive", async () => {
    const user = userEvent.setup();
    const onLaunchTerminal = vi.fn();
    const onOpenNewTaskModal = vi.fn();

    render(
      <WorktreesHistoryTab
        t={dummyT}
        language="zh"
        worktrees={mockWorktrees}
        loading={false}
        onLaunchTerminal={onLaunchTerminal}
        onOpenNewTaskModal={onOpenNewTaskModal}
      />
    );

    // Active one calls onLaunchTerminal with session_name
    const activeEnterBtn = screen.getByText("进入运行现场 (直连终端)");
    await user.click(activeEnterBtn);
    expect(onLaunchTerminal).toHaveBeenCalledWith("dev_one-space_feat-auth");

    // Offline one calls onLaunchTerminal with repo_name, branch
    const reviveBtns = screen.getAllByText("重新拉起现场 (直连终端)");
    await user.click(reviveBtns[0]);
    expect(onLaunchTerminal).toHaveBeenCalledWith("one-space", "fix-bug-123");
  });
});
