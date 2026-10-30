import { render, screen, fireEvent } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { PrPipelineTab } from "./components/PrPipelineTab";
import type { CloudAgentTask } from "@/lib/cloudAgent";

const dummyT = ((key: string, fallback?: string) => fallback ?? key) as any;

describe("PlanPipeline", () => {
  it("plan row shows no issue link/number, stage, commits, question, decision, PR", () => {
    const planTask: CloudAgentTask = {
      id: "job-1",
      repo: "r",
      issue_number: null as any,
      title: "Plan job",
      status: "waiting_input",
      started_at: 1,
      source: "plan",
      plan_id: "plan-1",
      workspace_id: "ws-1",
      stage: "waiting_input",
      base_commit: "abcdef1234567890",
      final_commit: "1234567890abcdef",
      question: { summary: "Need approval?" },
      decision: { text: "approved" },
      pr_url: "https://github.com/o/r/pull/9",
    };
    render(
      <PrPipelineTab
        t={dummyT}
        language="zh"
        tasks={[planTask]}
        loading={false}
        onRefresh={vi.fn()}
        onPlanAttach={vi.fn()}
        onPlanCancel={vi.fn()}
      />
    );
    expect(screen.queryByTestId("pipeline-task-issue-link")).not.toBeInTheDocument();
    expect(screen.getByText(/waiting_input|waiting input/i)).toBeInTheDocument();
    expect(screen.getByText(/abcdef1/)).toBeInTheDocument();
    expect(screen.getByText(/Need approval/)).toBeInTheDocument();
    expect(screen.getByText(/approved/)).toBeInTheDocument();
    expect(screen.getByText(/pull\/9/)).toBeInTheDocument();
  });

  it("takeover calls onPlanAttach", () => {
    const onPlanAttach = vi.fn();
    const planTask: CloudAgentTask = {
      id: "job-1",
      repo: "r",
      issue_number: null as any,
      title: "Plan job",
      status: "running",
      started_at: 1,
      source: "plan",
    };
    render(
      <PrPipelineTab
        t={dummyT}
        language="zh"
        tasks={[planTask]}
        loading={false}
        onRefresh={vi.fn()}
        onPlanAttach={onPlanAttach}
        onPlanCancel={vi.fn()}
      />
    );
    const btn = screen.getByTestId("plan-task-takeover") || screen.getByText(/takeover|接管/i);
    fireEvent.click(btn);
    expect(onPlanAttach).toHaveBeenCalledWith("job-1");
  });

  it("cancel calls onPlanCancel for active, unavailable for terminal", () => {
    const onPlanCancel = vi.fn();
    const activeTask: CloudAgentTask = { id: "job-1", repo: "r", issue_number: null as any, title: "Plan job", status: "running", started_at: 1, source: "plan" };
    const terminalTask: CloudAgentTask = { id: "job-2", repo: "r", issue_number: null as any, title: "Done", status: "completed", started_at: 1, source: "plan" };
    render(
      <PrPipelineTab
        t={dummyT}
        language="zh"
        tasks={[activeTask, terminalTask]}
        loading={false}
        onRefresh={vi.fn()}
        onPlanAttach={vi.fn()}
        onPlanCancel={onPlanCancel}
      />
    );
    const cancelBtn = screen.getAllByTestId("plan-task-cancel")[0];
    fireEvent.click(cancelBtn);
    expect(onPlanCancel).toHaveBeenCalledWith("job-1");
    // terminal cancel not available
    expect(screen.getAllByTestId("plan-task-cancel").length).toBe(1);
  });

  it("issue rows still render issue label/link", () => {
    const issueTask: CloudAgentTask = {
      id: "task-1",
      repo: "owner/repo",
      issue_number: 42,
      title: "Issue job",
      status: "completed",
      started_at: 1,
      pr_url: "https://github.com/owner/repo/pull/1",
    };
    render(
      <PrPipelineTab
        t={dummyT}
        language="zh"
        tasks={[issueTask]}
        loading={false}
        onRefresh={vi.fn()}
      />
    );
    expect(screen.getByTestId("pipeline-task-issue-link")).toBeInTheDocument();
  });
});
