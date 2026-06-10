import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import { AiFlow } from "@/components/AiFlow";
import { renderWithProviders } from "@/test/mocks/render";
import { dialogOpenMock, invokeMock, resetTauriMocks } from "@/test/mocks/tauri";

const apiMeta = { schema_version: 1, revision: 1 };

function mockAiFlowState() {
  invokeMock.mockImplementation(async (command: string, args?: any) => {
    if (command === "ai_flow_health_check") {
      return {
        ok: true,
        data: {
          installed: true,
          repo_commit: "abcdef123456",
          repo_branch: "main",
          items: [
            { id: "runtime", label: "Runtime", ok: true, status: "ok", path: "/tmp/ai-flow" },
          ],
        },
        meta: apiMeta,
      };
    }
    if (command === "ai_flow_install_latest") {
      return {
        ok: true,
        data: {
          repo_url: "https://example.test/ai-flow.git",
          cache_dir: "/tmp/cache",
          installed: true,
          commit: "fedcba987654",
          branch: "main",
          log: "installed",
        },
        meta: apiMeta,
      };
    }
    if (command === "ai_flow_projects_list") {
      return {
        ok: true,
        data: [
          {
            id: "/tmp/project-a",
            name: "Project A",
            root_path: "/tmp/project-a",
            ai_flow_dir: "/tmp/project-a/.ai-flow",
            from_workspace: true,
            has_ai_flow: true,
            plan_count: 2,
            pending_count: 1,
            failed_count: 0,
            done_count: 1,
            invalid_state_count: 0,
            queue_count: 1,
            group_count: 1,
            html_status_path: "/tmp/project-a/.ai-flow/html/index.html",
            updated_at: "2026-06-08T10:00:00+08:00",
          },
          {
            id: "/tmp/project-b",
            name: "Project B",
            root_path: "/tmp/project-b",
            ai_flow_dir: "/tmp/project-b/.ai-flow",
            from_workspace: true,
            has_ai_flow: false,
            plan_count: 0,
            pending_count: 0,
            failed_count: 0,
            done_count: 0,
            invalid_state_count: 0,
            queue_count: 0,
            group_count: 0,
            html_status_path: null,
            updated_at: "2026-06-08T11:00:00+08:00",
          },
        ],
        meta: apiMeta,
      };
    }
    if (command === "ai_flow_project_status") {
      expect(args).toEqual({ projectRoot: "/tmp/project-a" });
      return {
        ok: true,
        data: {
          project: {
            id: "/tmp/project-a",
            name: "Project A",
            root_path: "/tmp/project-a",
            ai_flow_dir: "/tmp/project-a/.ai-flow",
            from_workspace: true,
            has_ai_flow: true,
            plan_count: 2,
            pending_count: 1,
            failed_count: 0,
            done_count: 1,
            invalid_state_count: 0,
            queue_count: 1,
            group_count: 1,
            html_status_path: "/tmp/project-a/.ai-flow/html/index.html",
            updated_at: "2026-06-08T10:00:00+08:00",
          },
          plans: [
            {
              slug: "plan-a",
              title: "Plan A",
              current_status: "AWAITING_PLAN_REVIEW",
              plan_file: "docs/plan-a.md",
              plan_path: "/tmp/project-a/docs/plan-a.md",
              review_files: ["/tmp/project-a/.ai-flow/reports/plan-a-review.md"],
              created_at: "2026-06-08T09:00:00+08:00",
              updated_at: "2026-06-08T10:00:00+08:00",
              transitions: [
                { seq: 1, at: "2026-06-08T10:00:00+08:00", event: "created", from: null, to: "PENDING", note: "ready" },
              ],
              raw_state_path: "/tmp/project-a/.ai-flow/state/plan-a.json",
            },
            {
              slug: "plan-b",
              title: "Plan B",
              current_status: "PLANNED",
              plan_file: "docs/plan-b.md",
              plan_path: "/tmp/project-a/docs/plan-b.md",
              review_files: [],
              created_at: "2026-06-07T09:00:00+08:00",
              updated_at: "2026-06-07T10:00:00+08:00",
              transitions: [],
              raw_state_path: "/tmp/project-a/.ai-flow/state/plan-b.json",
            },
            {
              slug: "plan-c",
              title: "Plan C",
              current_status: "AWAITING_REVIEW",
              plan_file: "docs/plan-c.md",
              plan_path: "/tmp/project-a/docs/plan-c.md",
              review_files: [],
              created_at: "2026-06-06T09:00:00+08:00",
              updated_at: "2026-06-06T10:00:00+08:00",
              transitions: [],
              raw_state_path: "/tmp/project-a/.ai-flow/state/plan-c.json",
            },
            {
              slug: "plan-d",
              title: "Plan D",
              current_status: "DONE",
              plan_file: "docs/plan-d.md",
              plan_path: "/tmp/project-a/docs/plan-d.md",
              review_files: [],
              created_at: "2026-06-05T09:00:00+08:00",
              updated_at: "2026-06-05T10:00:00+08:00",
              transitions: [],
              raw_state_path: "/tmp/project-a/.ai-flow/state/plan-d.json",
            },
          ],
          queues: [
            { slug: "queue-a", title: "Queue A", current_status: "READY", items: [], raw_state_path: "/tmp/project-a/.ai-flow/orchestrations/state/queue-a.json" },
          ],
          groups: [
            { slug: "group-a", title: "Group A", current_status: "READY", current_child: null, children: [], dependencies: [], raw_state_path: "/tmp/project-a/.ai-flow/plan-groups/state/group-a.json" },
          ],
          invalid_states: [],
          config_summary: {
            global_setting_exists: true,
            project_setting_exists: false,
            project_rule_exists: true,
            effective_setting: {},
          },
          html_status_path: "/tmp/project-a/.ai-flow/html/index.html",
        },
        meta: apiMeta,
      };
    }
    if (command === "ai_flow_plan_content_get") {
      return {
        ok: true,
        data: {
          plan_path: "/tmp/project-a/docs/plan-a.md",
          content: "# Plan A\n\nImplement layout.",
          exists: true,
          error: null,
        },
        meta: apiMeta,
      };
    }
    if (command === "ai_flow_config_get") {
      return {
        ok: true,
        data: {
          scope: "project_rule",
          format: "yaml",
          path: "/tmp/project-a/.ai-flow/rule.yaml",
          exists: true,
          content: "version: 1\n",
        },
        meta: apiMeta,
      };
    }
    if (command === "ai_flow_open_path") return { ok: true, data: { opened: true }, meta: apiMeta };
    if (command === "ai_flow_launch_preview") {
      return {
        ok: true,
        data: { tool: "claude", permission_confirmation_required: false, prompt: "/ai-flow-plan-coding plan-a" },
        meta: apiMeta,
      };
    }
    if (command === "ai_flow_launch_action") return { ok: true, data: {}, meta: apiMeta };
    throw new Error(`Unhandled command: ${command} ${JSON.stringify(args)}`);
  });
}

describe("AiFlow", () => {
  beforeEach(() => {
    resetTauriMocks();
    mockAiFlowState();
    dialogOpenMock.mockResolvedValue("/tmp/project-a");
  });

  it("opens project detail from project cards and returns to the project list", async () => {
    const user = userEvent.setup();
    renderWithProviders(<AiFlow isVisible />);

    await user.click(await screen.findByText("Project A"));
    expect(await screen.findByRole("button", { name: /Back to projects|返回/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /^(Plans|计划)$/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /^(Queues|队列)$/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /^(Plan Groups|计划分组)$/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Config|配置/i })).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: /Back to projects|返回项目列表/i }));
    expect(await screen.findByText("Project A")).toBeInTheDocument();
  });

  it("adds an AI Flow working directory through folder selection instead of path input", async () => {
    const user = userEvent.setup();
    renderWithProviders(<AiFlow isVisible />);

    expect(await screen.findByText("Project A")).toBeInTheDocument();
    expect(screen.queryByPlaceholderText(/Import project path|导入项目路径/i)).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: /Add Working Directory|新增工作目录/i }));
    const dialog = await screen.findByRole("dialog", { name: /Add Working Directory|新增工作目录/i });
    await user.click(within(dialog).getByTitle(/Browse|浏览/i));
    await user.click(within(dialog).getByRole("button", { name: /^Add$|^添加$/i }));

    await waitFor(() => {
      expect(dialogOpenMock).toHaveBeenCalledWith({ directory: true, multiple: false });
      expect(invokeMock).toHaveBeenCalledWith("ai_flow_projects_list", {
        extraPath: "/tmp/project-a",
      });
    });
  });

  it("opens plan detail, reads content, and restores list view on back", async () => {
    const user = userEvent.setup();
    renderWithProviders(<AiFlow isVisible />);

    await user.click(await screen.findByText("Project A"));
    const list = await screen.findByTestId("ai-flow-plan-list");
    Object.defineProperty(list, "scrollTop", { value: 120, writable: true });
    await user.click(screen.getByRole("button", { name: /Plan A/i }));

    expect(await screen.findByText(/# Plan A/)).toBeInTheDocument();
    expect(invokeMock).toHaveBeenCalledWith("ai_flow_plan_content_get", {
      projectRoot: "/tmp/project-a",
      planSlug: "plan-a",
    });

    await user.click(screen.getByRole("button", { name: /Back to Plan list|返回 Plan 列表/i }));
    expect(await screen.findByTestId("ai-flow-plan-list")).toBeInTheDocument();
  });

  it("shows plan list quick actions according to current status", async () => {
    const user = userEvent.setup();
    renderWithProviders(<AiFlow isVisible />);

    await user.click(await screen.findByText("Project A"));
    const planList = await screen.findByTestId("ai-flow-plan-list");

    const planA = within(planList).getByRole("button", { name: /Plan A/i }).closest(".rounded-lg");
    const planB = within(planList).getByRole("button", { name: /Plan B/i }).closest(".rounded-lg");
    const planC = within(planList).getByRole("button", { name: /Plan C/i }).closest(".rounded-lg");
    const planD = within(planList).getByRole("button", { name: /Plan D/i }).closest(".rounded-lg");

    expect(within(planA as HTMLElement).getByRole("button", { name: /Plan review|Plan 审核/i })).toBeInTheDocument();
    expect(within(planA as HTMLElement).queryByRole("button", { name: /Coding|编码/i })).not.toBeInTheDocument();
    expect(within(planA as HTMLElement).queryByRole("button", { name: /^Review$|代码审核/i })).not.toBeInTheDocument();

    expect(within(planB as HTMLElement).getByRole("button", { name: /Coding|编码/i })).toBeInTheDocument();
    expect(within(planC as HTMLElement).getByRole("button", { name: /^Review$|代码审核/i })).toBeInTheDocument();
    expect(within(planD as HTMLElement).queryByRole("button", { name: /Plan review|Plan 审核|Coding|编码|^Review$|代码审核/i })).not.toBeInTheDocument();
  });

  it("shows queues, plan groups, and config inside project detail tabs", async () => {
    const user = userEvent.setup();
    renderWithProviders(<AiFlow isVisible />);

    await user.click(await screen.findByText("Project A"));
    await user.click(screen.getByRole("button", { name: /^(Queues|队列)$/i }));
    expect(await screen.findByText("Queue A")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: /^(Plan Groups|计划分组)$/i }));
    expect(await screen.findByText("Group A")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: /Config|配置/i }));
    expect(await screen.findByRole("heading", { name: /Configuration|配置/i })).toBeInTheDocument();
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("ai_flow_config_get", {
      scope: "project_rule",
      projectRoot: "/tmp/project-a",
    }));
  });

  it("opens install and health dialog and runs refresh/install actions", async () => {
    const user = userEvent.setup();
    renderWithProviders(<AiFlow isVisible />);

    await user.click(await screen.findByRole("button", { name: /Install and Health|安装与健康/i }));
    const dialog = await screen.findByRole("dialog", { name: /Install and Health Check|安装与健康检查/i });
    expect(within(dialog).getByText("Runtime")).toBeInTheDocument();

    await user.click(within(dialog).getByRole("button", { name: /Run health check|运行健康检查/i }));
    await user.click(within(dialog).getByRole("button", { name: /Install latest|安装最新版/i }));

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("ai_flow_health_check");
      expect(invokeMock).toHaveBeenCalledWith("ai_flow_install_latest");
    });
  });

  it("renders only the project directory row and quick open buttons", async () => {
    renderWithProviders(<AiFlow isVisible />);

    const projectTitle = await screen.findByText("Project A");
    const card = projectTitle.closest("article");
    expect(card).not.toBeNull();
    expect(within(card as HTMLElement).getByText(/Project dir|项目目录/i)).toBeInTheDocument();
    expect(within(card as HTMLElement).getByTitle("/tmp/project-a")).toHaveTextContent("/tmp/project-a");
    expect(within(card as HTMLElement).queryByText(/^AI Flow dir$|^AI Flow 目录$/i)).not.toBeInTheDocument();
    expect(within(card as HTMLElement).queryByText(/^Status page$|^状态页目录$/i)).not.toBeInTheDocument();
    expect(within(card as HTMLElement).getByRole("button", { name: /Open AI Flow dir|打开 AI Flow 目录/i })).toBeInTheDocument();
    expect(within(card as HTMLElement).getByRole("button", { name: /Open status dir|打开状态目录/i })).toBeInTheDocument();
  });

  it("opens AI Flow and status directories without navigating to project detail", async () => {
    const user = userEvent.setup();
    renderWithProviders(<AiFlow isVisible />);

    const projectTitle = await screen.findByText("Project A");
    const card = projectTitle.closest("article");
    expect(card).not.toBeNull();

    await user.click(within(card as HTMLElement).getByRole("button", { name: /Open AI Flow dir|打开 AI Flow 目录/i }));
    await user.click(within(card as HTMLElement).getByRole("button", { name: /Open status dir|打开状态目录/i }));

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("ai_flow_open_path", { path: "/tmp/project-a/.ai-flow" });
      expect(invokeMock).toHaveBeenCalledWith("ai_flow_open_path", { path: "/tmp/project-a/.ai-flow/html" });
    });
    expect(screen.queryByRole("button", { name: /Back to projects|返回项目列表/i })).not.toBeInTheDocument();
    expect(invokeMock).not.toHaveBeenCalledWith("ai_flow_project_status", { projectRoot: "/tmp/project-a" });
  });

  it("shows uninitialized workspace project without allowing detail navigation", async () => {
    const user = userEvent.setup();
    renderWithProviders(<AiFlow isVisible />);

    const projectTitle = await screen.findByText("Project B");
    const card = projectTitle.closest("article");
    expect(card).not.toBeNull();
    expect(within(card as HTMLElement).getByText(/Not initialized|未初始化/i)).toBeInTheDocument();
    expect(within(card as HTMLElement).getByText(/Pending|待处理/i)).toBeInTheDocument();
    expect(within(card as HTMLElement).getByText(/Failed|失败/i)).toBeInTheDocument();
    expect(within(card as HTMLElement).getByText(/Invalid|无效/i)).toBeInTheDocument();
    expect(within(card as HTMLElement).getByText(/Plans 0|计划 0/i)).toBeInTheDocument();
    expect(within(card as HTMLElement).getByText(/Queues 0|队列 0/i)).toBeInTheDocument();
    expect(within(card as HTMLElement).getByText(/Groups 0|分组 0/i)).toBeInTheDocument();
    expect(within(card as HTMLElement).getByText(/Done 0|完成 0/i)).toBeInTheDocument();
    expect(within(card as HTMLElement).queryByRole("button", { name: /Open AI Flow dir|打开 AI Flow 目录/i })).not.toBeInTheDocument();
    expect(within(card as HTMLElement).queryByRole("button", { name: /Open status dir|打开状态目录/i })).not.toBeInTheDocument();

    await user.click(card as HTMLElement);
    expect(screen.queryByRole("button", { name: /Back to projects|返回项目列表/i })).not.toBeInTheDocument();
    expect(invokeMock).not.toHaveBeenCalledWith("ai_flow_project_status", { projectRoot: "/tmp/project-b" });
  });
});
