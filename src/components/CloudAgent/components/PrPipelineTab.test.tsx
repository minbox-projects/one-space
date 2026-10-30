import { render, screen, fireEvent, act } from "@testing-library/react";
import { describe, expect, it, vi, beforeEach } from "vitest";
import {
  PrPipelineTab,
  resolveIssueUrl,
  derivePipelineSteps,
} from "./PrPipelineTab";
import type { CloudAgentTask } from "@/lib/cloudAgent";

describe("PrPipelineTab Component", () => {
  const dummyT = ((key: string, fallback?: string | Record<string, unknown>) => {
    if (typeof fallback === "string") return fallback;
    if (fallback && typeof fallback === "object" && "defaultValue" in fallback) {
      return (fallback as { defaultValue?: string }).defaultValue ?? key;
    }
    return key;
  }) as any;

  const mockTasks: CloudAgentTask[] = [
    {
      id: "task-1",
      repo: "one-space",
      issue_number: 101,
      title: "Fix websocket reconnect issue",
      branch: "fix-ws-reconnect",
      status: "completed",
      pr_url: "https://github.com/example/one-space/pull/101",
      started_at: 1700000000,
      duration_seconds: 45,
      error_message: null,
      author: "octocat",
      trigger_type: "first_run",
      attempts: 1,
      max_attempts: 3,
      diff_stat: "2 files changed, +45 -12",
      step: "done",
    },
    {
      id: "task-2",
      repo: "minbox-projects/onespace-app",
      issue_number: 102,
      title: "Refactor terminal session runner",
      branch: "feat-terminal-runner",
      status: "running",
      started_at: 1700001000,
      duration_seconds: null,
      error_message: null,
      author: "developer",
      trigger_type: "follow_up",
      attempts: 2,
      max_attempts: 3,
      step: "opencode",
    },
    {
      id: "task-3",
      repo: "https://github.com/my-org/custom-repo.git",
      issue_number: 103,
      title: "Automate release checks and tags",
      branch: "chore-release",
      status: "no_changes",
      started_at: 1700002000,
      duration_seconds: 12,
      error_message: null,
      trigger_type: "first_run",
    },
    {
      id: "task-4",
      repo: "onespace-cloud-agent",
      issue_number: 104,
      title: "Handle long error messages in pipeline worker",
      branch: "fix-error-handling",
      status: "failed",
      started_at: 1700003000,
      duration_seconds: 120,
      error_message:
        "Traceback (most recent call last):\n  File 'issue_worker.py', line 245, in execute_task\n    raise subprocess.CalledProcessError(returncode=1, cmd='opencode run')\nCalledProcessError: Command '['opencode', 'run']' returned non-zero exit status 1.",
      attempts: 3,
      max_attempts: 3,
      step: "failed",
    },
  ];

  beforeEach(() => {
    Object.defineProperty(navigator, "clipboard", {
      value: {
        writeText: vi.fn().mockResolvedValue(undefined),
      },
      configurable: true,
      writable: true,
    });
  });

  describe("resolveIssueUrl helper", () => {
    it("derives issue url from pr_url when available", () => {
      const url = resolveIssueUrl({
        id: "t1",
        repo: "one-space",
        issue_number: 42,
        title: "Test",
        status: "completed",
        started_at: 100,
        pr_url: "https://github.com/owner/one-space/pull/99",
      });
      expect(url).toBe("https://github.com/owner/one-space/issues/42");
    });

    it("derives issue url from owner/repo format", () => {
      const url = resolveIssueUrl({
        id: "t2",
        repo: "facebook/react",
        issue_number: 1234,
        title: "Test",
        status: "running",
        started_at: 100,
      });
      expect(url).toBe("https://github.com/facebook/react/issues/1234");
    });

    it("derives issue url from https git repo url", () => {
      const url = resolveIssueUrl({
        id: "t3",
        repo: "https://github.com/vuejs/core.git",
        issue_number: 56,
        title: "Test",
        status: "running",
        started_at: 100,
      });
      expect(url).toBe("https://github.com/vuejs/core/issues/56");
    });

    it("derives issue url from ssh git repo format", () => {
      const url = resolveIssueUrl({
        id: "t4",
        repo: "git@github.com:torvalds/linux.git",
        issue_number: 789,
        title: "Test",
        status: "running",
        started_at: 100,
      });
      expect(url).toBe("https://github.com/torvalds/linux/issues/789");
    });

    it("returns null when issue_number is missing", () => {
      const url = resolveIssueUrl({
        id: "t5",
        repo: "torvalds/linux",
        issue_number: 0,
        title: "Test",
        status: "running",
        started_at: 100,
      });
      expect(url).toBeNull();
    });
  });

  describe("derivePipelineSteps helper", () => {
    it("returns all completed steps for completed status", () => {
      const steps = derivePipelineSteps(mockTasks[0], dummyT);
      expect(steps.every((s) => s.state === "completed")).toBe(true);
    });

    it("marks skipped step for no_changes status", () => {
      const steps = derivePipelineSteps(mockTasks[2], dummyT);
      expect(steps[3].state).toBe("skipped");
    });

    it("marks current step for running status", () => {
      const steps = derivePipelineSteps(mockTasks[1], dummyT);
      expect(steps.some((s) => s.state === "current")).toBe(true);
    });

    it("marks failed step for failed status", () => {
      const steps = derivePipelineSteps(mockTasks[3], dummyT);
      expect(steps.some((s) => s.state === "failed")).toBe(true);
    });
  });

  it("renders empty state and responds to refresh action", () => {
    const onRefresh = vi.fn();

    render(
      <PrPipelineTab
        t={dummyT}
        language="zh"
        tasks={[]}
        loading={false}
        onRefresh={onRefresh}
      />
    );

    expect(screen.getByText("暂无 Issues 自动流水线任务记录")).toBeInTheDocument();
    const refreshBtn = screen.getByText("刷新任务流水");
    fireEvent.click(refreshBtn);
    expect(onRefresh).toHaveBeenCalledTimes(1);
  });

  it("renders loading spinner when loading with empty tasks", () => {
    const { container } = render(
      <PrPipelineTab
        t={dummyT}
        language="zh"
        tasks={[]}
        loading={true}
        onRefresh={vi.fn()}
      />
    );

    expect(container.querySelector(".animate-spin")).toBeTruthy();
  });

  it("renders tasks with various statuses, badges, trigger tags and PR pills", () => {
    render(
      <PrPipelineTab
        t={dummyT}
        language="zh"
        tasks={mockTasks}
        loading={false}
        onRefresh={vi.fn()}
      />
    );

    // Header count
    expect(screen.getByText("近期任务流转历史 (4)")).toBeInTheDocument();

    // Status badges
    expect(screen.getByText("PR 已创建")).toBeInTheDocument();
    expect(screen.getByText("实施中")).toBeInTheDocument();
    expect(screen.getByText("无变更")).toBeInTheDocument();
    expect(screen.getByText("执行失败")).toBeInTheDocument();

    // Trigger tags
    expect(screen.getAllByText("标签接单").length).toBeGreaterThanOrEqual(1);
    expect(screen.getByText("追加指令")).toBeInTheDocument();

    // Attempt badges
    expect(screen.getByText("尝试 1/3")).toBeInTheDocument();
    expect(screen.getByText("尝试 2/3")).toBeInTheDocument();

    // Repo and issue labels (hides front prefix, displays tail repo and #issue full)
    expect(screen.getByText("one-space#101")).toBeInTheDocument();
    expect(screen.getByText("…/onespace-app#102")).toBeInTheDocument();

    // Branch tags
    expect(screen.getByText("fix-ws-reconnect")).toBeInTheDocument();
    expect(screen.getByText("feat-terminal-runner")).toBeInTheDocument();

    // Author and Diff Stat
    expect(screen.getByText("@octocat")).toBeInTheDocument();
    expect(screen.getByText("2 files changed, +45 -12")).toBeInTheDocument();

    // Titles
    expect(screen.getByText("Fix websocket reconnect issue")).toBeInTheDocument();
    expect(screen.getByText("Refactor terminal session runner")).toBeInTheDocument();

    // PR Pill & Link
    const prBadges = screen.getAllByTestId("pipeline-task-pr-badge");
    expect(prBadges.length).toBe(1);
    expect(prBadges[0]).toHaveAttribute(
      "href",
      "https://github.com/example/one-space/pull/101"
    );
    expect(prBadges[0]).toHaveTextContent("PR #101");

    // Duration friendly
    expect(screen.getByText("耗时 45秒")).toBeInTheDocument();
    expect(screen.getByText("耗时 2分")).toBeInTheDocument();
  });

  it("links both the repo#issue tag and title to the resolved issue page", () => {
    render(
      <PrPipelineTab
        t={dummyT}
        language="zh"
        tasks={mockTasks}
        loading={false}
        onRefresh={vi.fn()}
      />
    );

    const issueLinks = screen.getAllByTestId("pipeline-task-issue-link");
    expect(issueLinks.length).toBe(4);

    // task-1 derives from pr_url
    expect(issueLinks[0]).toHaveAttribute(
      "href",
      "https://github.com/example/one-space/issues/101"
    );
    expect(issueLinks[0]).toHaveAttribute("target", "_blank");

    // task-2 derives from repo
    expect(issueLinks[1]).toHaveAttribute(
      "href",
      "https://github.com/minbox-projects/onespace-app/issues/102"
    );

    // task-3 derives from https repo url
    expect(issueLinks[2]).toHaveAttribute(
      "href",
      "https://github.com/my-org/custom-repo/issues/103"
    );

    // Task titles as links
    const titleLinks = screen.getAllByTestId("pipeline-task-title-link");
    expect(titleLinks.length).toBe(4);
    expect(titleLinks[0]).toHaveAttribute(
      "href",
      "https://github.com/example/one-space/issues/101"
    );
    expect(titleLinks[0]).toHaveTextContent("Fix websocket reconnect issue");
  });

  it("renders pipeline steps progress bar for each task", () => {
    render(
      <PrPipelineTab
        t={dummyT}
        language="zh"
        tasks={mockTasks}
        loading={false}
        onRefresh={vi.fn()}
      />
    );

    const stepFlows = screen.getAllByTestId("pipeline-steps-flow");
    expect(stepFlows.length).toBe(4);
    expect(screen.getAllByText("接单鉴权").length).toBe(4);
    expect(screen.getAllByText("创建 Worktree").length).toBe(4);
    expect(screen.getAllByText("实施").length).toBe(4);
    expect(screen.getAllByText("推送").length).toBe(4);
    expect(screen.getAllByText("PR 创建").length).toBe(4);
  });

  it("handles collapsible error box and copies error traceback", () => {
    const writeTextSpy = vi
      .spyOn(navigator.clipboard, "writeText")
      .mockResolvedValue(undefined);

    render(
      <PrPipelineTab
        t={dummyT}
        language="zh"
        tasks={mockTasks}
        loading={false}
        onRefresh={vi.fn()}
      />
    );

    const errorBox = screen.getByTestId("pipeline-task-error-box");
    expect(errorBox).toBeInTheDocument();
    expect(errorBox).toHaveTextContent("异常");

    // Initially collapsed
    expect(screen.queryByTestId("pipeline-task-error-expanded")).not.toBeInTheDocument();

    // Toggle expand
    const expandBtn = screen.getByText("展开详情");
    fireEvent.click(expandBtn);
    expect(screen.getByTestId("pipeline-task-error-expanded")).toBeInTheDocument();
    expect(screen.getByTestId("pipeline-task-error-expanded")).toHaveTextContent("Traceback");

    // Copy error
    const copyErrorBtn = screen.getByTitle("复制完整错误");
    fireEvent.click(copyErrorBtn);
    expect(writeTextSpy).toHaveBeenCalledWith(mockTasks[3].error_message);
    expect(screen.getByText("已复制")).toBeInTheDocument();

    // Toggle collapse
    const collapseBtn = screen.getByText("收起");
    fireEvent.click(collapseBtn);
    expect(screen.queryByTestId("pipeline-task-error-expanded")).not.toBeInTheDocument();
  });

  it("copies git checkout command and launches terminal on click", async () => {
    const writeTextSpy = vi
      .spyOn(navigator.clipboard, "writeText")
      .mockResolvedValue(undefined);
    const onLaunchTerminal = vi.fn().mockResolvedValue(undefined);

    render(
      <PrPipelineTab
        t={dummyT}
        language="zh"
        tasks={mockTasks}
        loading={false}
        onRefresh={vi.fn()}
        onLaunchTerminal={onLaunchTerminal}
      />
    );

    // Copy checkout command
    const copyCheckoutBtns = screen.getAllByTestId("pipeline-task-copy-checkout");
    expect(copyCheckoutBtns.length).toBeGreaterThanOrEqual(1);

    fireEvent.click(copyCheckoutBtns[0]);
    expect(writeTextSpy).toHaveBeenCalledWith(
      "git fetch origin fix-ws-reconnect && git checkout fix-ws-reconnect"
    );

    // Launch terminal
    const terminalBtns = screen.getAllByTestId("pipeline-task-terminal-btn");
    expect(terminalBtns.length).toBe(4);

    // Verify terminal icon has white color
    const terminalIcon = terminalBtns[0].querySelector("svg");
    expect(terminalIcon).toHaveClass("text-white");

    // Verify terminal button is aligned to the right while duration is in the same row
    const rowContainer = terminalBtns[0].closest(".flex.items-center.justify-between");
    expect(rowContainer).toBeInTheDocument();
    expect(rowContainer).toHaveTextContent("耗时 45秒");

    await act(async () => {
      fireEvent.click(terminalBtns[0]);
    });
    expect(onLaunchTerminal).toHaveBeenCalledWith(
      "one-space",
      "fix-ws-reconnect"
    );
  });
});
