import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import type { CloudAgentTask } from "./cloudAgent";
import {
  workspacesList,
  cloudAgentPlanContext,
  cloudAgentPlanList,
  cloudAgentPlanSubmit,
  cloudAgentPlanCancel,
  cloudAgentPlanAttach,
  cloudAgentPlanCapabilities,
} from "./cloudAgent";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;

describe("cloudAgent plan wrappers", () => {
  beforeEach(() => invokeMock.mockReset());

  it("workspacesList unwraps .data", async () => {
    invokeMock.mockResolvedValue({ data: [{ id: "w1" }] });
    const res = await workspacesList();
    expect(invokeMock).toHaveBeenCalledWith("workspaces_list");
    expect(res).toEqual([{ id: "w1" }]);
  });

  it("cloudAgentPlanContext", async () => {
    invokeMock.mockResolvedValue({ plans: [] });
    await cloudAgentPlanContext("ws-1");
    expect(invokeMock).toHaveBeenCalledWith("cloud_agent_plan_context", { workspaceId: "ws-1" });
  });

  it("cloudAgentPlanList", async () => {
    invokeMock.mockResolvedValue([]);
    await cloudAgentPlanList("ws-1");
    expect(invokeMock).toHaveBeenCalledWith("cloud_agent_plan_list", { workspaceId: "ws-1" });
  });

  it("cloudAgentPlanSubmit", async () => {
    invokeMock.mockResolvedValue({ task: {}, reused: false, request_id: "r1" });
    const input = { workspace_id: "ws-1", plan_directory: "/p", base_branch: "main", request_id: "r1", host: "h" };
    await cloudAgentPlanSubmit(input as any);
    expect(invokeMock).toHaveBeenCalledWith("cloud_agent_plan_submit", { input });
  });

  it("cloudAgentPlanCancel", async () => {
    invokeMock.mockResolvedValue(undefined);
    await cloudAgentPlanCancel("job-1", "h");
    expect(invokeMock).toHaveBeenCalledWith("cloud_agent_plan_cancel", { host: "h", jobId: "job-1" });
  });

  it("cloudAgentPlanAttach", async () => {
    invokeMock.mockResolvedValue(undefined);
    await cloudAgentPlanAttach("job-1");
    expect(invokeMock).toHaveBeenCalledWith("cloud_agent_plan_attach", { host: undefined, jobId: "job-1" });
  });

  it("cloudAgentPlanCapabilities", async () => {
    invokeMock.mockResolvedValue({});
    await cloudAgentPlanCapabilities("h");
    expect(invokeMock).toHaveBeenCalledWith("cloud_agent_plan_capabilities", { host: "h" });
  });
});

describe("CloudAgentTask plan sample type-checks", () => {
  it("accepts plan task without issue number", () => {
    const task: CloudAgentTask = {
      id: "j1",
      repo: "r",
      issue_number: null,
      title: "t",
      status: "running",
      started_at: 1,
      source: "plan",
      plan_id: "p1",
      plan_title: "P",
      workspace_id: "w1",
      base_branch: "main",
      base_commit: "abc",
      final_commit: null,
      session_id: "s",
      session_url: "u",
      stage: "running",
      question: { q: 1 },
      decision: { d: 1 },
      remote_root: "/r",
      local_root: "/l",
      updated_at: 2,
    };
    expect(task.source).toBe("plan");
  });
});
