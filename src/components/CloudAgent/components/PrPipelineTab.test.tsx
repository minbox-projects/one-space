import { render, screen, fireEvent } from "@testing-library/react";
import { describe, expect, it, vi, beforeEach } from "vitest";
import { PrPipelineTab, resolveIssueUrl } from "./PrPipelineTab";
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

  it("renders tasks with various statuses, branch tags and PR links", () => {
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

    // Repo and issue labels
    expect(screen.getByText("one-space#101")).toBeInTheDocument();
    expect(screen.getByText("minbox-projects/onespace-app#102")).toBeInTheDocument();

    // Branch tags
    expect(screen.getByText("fix-ws-reconnect")).toBeInTheDocument();
    expect(screen.getByText("feat-terminal-runner")).toBeInTheDocument();

    // Titles
    expect(screen.getByText("Fix websocket reconnect issue")).toBeInTheDocument();
    expect(screen.getByText("Refactor terminal session runner")).toBeInTheDocument();

    // PR Link
    const prLink = screen.getByText("查看 PR").closest("a");
    expect(prLink).toHaveAttribute(
      "href",
      "https://github.com/example/one-space/pull/101"
    );

    // Duration
    expect(screen.getByText("耗时 45 秒")).toBeInTheDocument();
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

  it("handles long error message with collapsed single-line view and tooltip preview", () => {
    render(
      <PrPipelineTab
        t={dummyT}
        language="zh"
        tasks={mockTasks}
        loading={false}
        onRefresh={vi.fn()}
      />
    );

    // Collapsed single line error is present with truncate
    const collapsedError = screen.getByTestId("pipeline-task-error-collapsed");
    expect(collapsedError).toHaveClass("truncate");
    expect(collapsedError.textContent).toContain("Traceback (most recent call last)");

    // Tooltip exists in DOM with role="tooltip"
    const tooltip = screen.getByTestId("pipeline-task-error-tooltip");
    expect(tooltip).toHaveAttribute("role", "tooltip");
    expect(tooltip).toHaveTextContent("错误详细信息");
    expect(tooltip.textContent).toContain("CalledProcessError: Command '['opencode', 'run']'");

    // Copy button inside tooltip
    const writeTextSpy = vi
      .spyOn(navigator.clipboard, "writeText")
      .mockResolvedValue(undefined);
    const copyBtn = screen.getByTitle("复制完整错误");
    expect(copyBtn).toBeInTheDocument();

    fireEvent.click(copyBtn);
    expect(writeTextSpy).toHaveBeenCalledWith(
      mockTasks[3].error_message
    );
    expect(screen.getByText("已复制错误信息")).toBeInTheDocument();
  });
});
