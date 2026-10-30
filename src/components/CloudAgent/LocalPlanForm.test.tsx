import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { LocalPlanForm } from "./components/LocalPlanForm";

const t = ((key: string, fallback?: string) => fallback ?? key) as any;

vi.mock("@/lib/cloudAgent", () => ({
  workspacesList: vi.fn(),
  cloudAgentPlanContext: vi.fn(),
  cloudAgentPlanSubmit: vi.fn(),
}));

import { workspacesList, cloudAgentPlanContext, cloudAgentPlanSubmit } from "@/lib/cloudAgent";

describe("LocalPlanForm", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    (workspacesList as any).mockResolvedValue([{ workspace: { id: "ws-1", name: "ws1", root_path: "/root/ws" } }]);
    (cloudAgentPlanContext as any).mockResolvedValue({
      repository: { repo: "o/r", base_branch: "main", source: "origin" },
      plans: [{ plan_id: "plan-a", title: "Plan A", status: "valid", valid: true, has_tasks: true }],
    });
  });

  it("loads workspaces and renders select", async () => {
    render(<LocalPlanForm t={t} host="h" onSubmitted={vi.fn()} />);
    await waitFor(() => expect(screen.getByTestId("local-plan-workspace-select")).toBeInTheDocument());
  });

  it("shows repository preview after selection", async () => {
    const user = userEvent.setup();
    render(<LocalPlanForm t={t} host="h" onSubmitted={vi.fn()} />);
    await waitFor(() => screen.getByTestId("local-plan-workspace-select"));
    await user.selectOptions(screen.getByTestId("local-plan-workspace-select"), "ws-1");
    await waitFor(() => expect(screen.getByTestId("local-plan-repository")).toBeInTheDocument());
    expect(screen.getByTestId("local-plan-base-branch")).toBeInTheDocument();
    expect(screen.getByTestId("local-plan-option-plan-a")).toBeInTheDocument();
  });

  it("invalid plan disabled with reason", async () => {
    (cloudAgentPlanContext as any).mockResolvedValue({
      repository: { repo: "o/r", base_branch: "main", source: "origin" },
      plans: [{ plan_id: "bad", title: "Bad", status: "invalid", valid: false, error: "missing tasks" }],
    });
    const user = userEvent.setup();
    render(<LocalPlanForm t={t} host="h" onSubmitted={vi.fn()} />);
    await waitFor(() => screen.getByTestId("local-plan-workspace-select"));
    await user.selectOptions(screen.getByTestId("local-plan-workspace-select"), "ws-1");
    await waitFor(() => screen.getByTestId("local-plan-option-bad"));
    expect(screen.getByTestId("local-plan-option-bad")).toHaveTextContent("missing tasks");
  });

  it("workspace change clears selection", async () => {
    (workspacesList as any).mockResolvedValue([
      { workspace: { id: "ws-1", name: "ws1", root_path: "/root/ws" } },
      { workspace: { id: "ws-2", name: "ws2", root_path: "/root/ws2" } },
    ]);
    const user = userEvent.setup();
    render(<LocalPlanForm t={t} host="h" onSubmitted={vi.fn()} />);
    await waitFor(() => screen.getByTestId("local-plan-workspace-select"));
    await user.selectOptions(screen.getByTestId("local-plan-workspace-select"), "ws-1");
    await waitFor(() => screen.getByTestId("local-plan-option-plan-a"));
    await user.click(screen.getByTestId("local-plan-option-plan-a"));
    await user.selectOptions(screen.getByTestId("local-plan-workspace-select"), "ws-2");
    expect(screen.getByTestId("local-plan-submit")).toBeDisabled();
  });

  it("consent text present", async () => {
    render(<LocalPlanForm t={t} host="h" onSubmitted={vi.fn()} />);
    await waitFor(() => screen.getByTestId("local-plan-workspace-select"));
    expect(screen.getByTestId("local-plan-consent")).toBeInTheDocument();
  });

  it("duplicate clicks single submit", async () => {
    (cloudAgentPlanSubmit as any).mockResolvedValue({ reused: false, request_id: "r1", task: {} });
    const user = userEvent.setup();
    render(<LocalPlanForm t={t} host="h" onSubmitted={vi.fn()} />);
    await waitFor(() => screen.getByTestId("local-plan-workspace-select"));
    await user.selectOptions(screen.getByTestId("local-plan-workspace-select"), "ws-1");
    await waitFor(() => screen.getByTestId("local-plan-option-plan-a"));
    await user.click(screen.getByTestId("local-plan-option-plan-a"));
    const btn = screen.getByTestId("local-plan-submit");
    await user.click(btn);
    await user.click(btn);
    await waitFor(() => expect(cloudAgentPlanSubmit).toHaveBeenCalledTimes(1));
  });

  it("reused shows notice", async () => {
    (cloudAgentPlanSubmit as any).mockResolvedValue({ reused: true, request_id: "r1", task: { id: "job-1" } });
    const onSubmitted = vi.fn();
    const user = userEvent.setup();
    render(<LocalPlanForm t={t} host="h" onSubmitted={onSubmitted} />);
    await waitFor(() => screen.getByTestId("local-plan-workspace-select"));
    await user.selectOptions(screen.getByTestId("local-plan-workspace-select"), "ws-1");
    await waitFor(() => screen.getByTestId("local-plan-option-plan-a"));
    await user.click(screen.getByTestId("local-plan-option-plan-a"));
    await user.click(screen.getByTestId("local-plan-submit"));
    await waitFor(() => expect(onSubmitted).toHaveBeenCalled());
  });

  it("visible upgrade error", async () => {
    (cloudAgentPlanSubmit as any).mockRejectedValue(new Error("upgrade required"));
    const user = userEvent.setup();
    render(<LocalPlanForm t={t} host="h" onSubmitted={vi.fn()} />);
    await waitFor(() => screen.getByTestId("local-plan-workspace-select"));
    await user.selectOptions(screen.getByTestId("local-plan-workspace-select"), "ws-1");
    await waitFor(() => screen.getByTestId("local-plan-option-plan-a"));
    await user.click(screen.getByTestId("local-plan-option-plan-a"));
    await user.click(screen.getByTestId("local-plan-submit"));
    await waitFor(() => expect(screen.getByTestId("local-plan-error")).toHaveTextContent("upgrade"));
  });

  it("host change ignores late context", async () => {
    let resolveLate: any;
    (cloudAgentPlanContext as any).mockImplementation(() => new Promise((res) => { resolveLate = res; }));
    const { rerender } = render(<LocalPlanForm t={t} host="h1" onSubmitted={vi.fn()} />);
    await waitFor(() => screen.getByTestId("local-plan-workspace-select"));
    rerender(<LocalPlanForm t={t} host="h2" onSubmitted={vi.fn()} />);
    resolveLate({ repository: { repo: "late", base_branch: "main", source: "origin" }, plans: [] });
    await new Promise((r) => setTimeout(r, 50));
    expect(screen.queryByText("late")).not.toBeInTheDocument();
  });
});
