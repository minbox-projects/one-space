import { fireEvent, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { AiWorkFlowTool } from "@/components/AiWorkFlowTool";
import { renderWithProviders } from "@/test/mocks/render";
import * as api from "@/lib/aiWorkFlow";

vi.mock("@/lib/aiWorkFlow", async (importOriginal) => {
  const original = await importOriginal<typeof import("@/lib/aiWorkFlow")>();
  return {
    ...original,
    aiWorkFlowInstallStatus: vi.fn(),
    aiWorkFlowInstallVersion: vi.fn(),
    aiWorkFlowInstallOrUpdate: vi.fn(),
    aiWorkFlowInstallCancel: vi.fn(),
    aiWorkFlowInstallLogs: vi.fn(),
    aiWorkFlowEnvironmentList: vi.fn(),
    aiWorkFlowEnvironmentCreate: vi.fn(),
    aiWorkFlowEnvironmentRead: vi.fn(),
    aiWorkFlowEnvironmentUpdate: vi.fn(),
    aiWorkFlowEnvironmentDelete: vi.fn(),
    aiWorkFlowEnvironmentUse: vi.fn(),
    aiWorkFlowEnvironmentStatus: vi.fn(),
  };
});

vi.mock("./ConfirmDialogProvider", async (importOriginal) => {
  const original = await importOriginal<typeof import("./ConfirmDialogProvider")>();
  return { ...original, useConfirmDialog: () => vi.fn().mockResolvedValue(true) };
});

const idleStatus: api.AiWorkFlowInstallStatus = {
  state: "idle",
  operation: null,
  stage: null,
  started_at: null,
  finished_at: null,
  version: null,
  error: null,
};

const teamDocument: api.AiWorkFlowEnvironmentDocument = {
  name: "team",
  content: "{\n  \"agents\": {},\n  \"custom\": true\n}\n",
  value: { agents: {}, custom: true },
  current: true,
  valid: true,
  validation_error: null,
};

describe("AiWorkFlowTool", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(api.aiWorkFlowInstallStatus).mockResolvedValue(idleStatus);
    vi.mocked(api.aiWorkFlowInstallVersion).mockResolvedValue({ installed: false, version: null, error: null });
    vi.mocked(api.aiWorkFlowInstallLogs).mockResolvedValue([]);
    vi.mocked(api.aiWorkFlowEnvironmentList).mockResolvedValue([]);
    vi.mocked(api.aiWorkFlowEnvironmentStatus).mockResolvedValue({ current: "default", exists: true, valid: true });
  });

  it("展示静态简介、安装状态和空环境，且不提供文档链接", async () => {
    renderWithProviders(<AiWorkFlowTool />);

    expect(screen.getByRole("heading", { name: "AI Work Flow" })).toBeInTheDocument();
    expect(await screen.findByText(/Not installed|未安装/)).toBeInTheDocument();
    expect(screen.getByText(/No saved environments|还没有已保存环境/)).toBeInTheDocument();
    expect(screen.queryByRole("link")).not.toBeInTheDocument();
    expect(screen.getByText("default")).toBeInTheDocument();
  });

  it("版本未知时仍识别为已安装并允许更新", async () => {
    vi.mocked(api.aiWorkFlowInstallVersion).mockResolvedValue({ installed: true, version: null, error: null });
    renderWithProviders(<AiWorkFlowTool />);

    expect(await screen.findByText(/Installed \(version unknown\)|已安装（版本未知）/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /^Update$|^更新$/ })).toBeEnabled();
  });

  it("运行时禁用重复安装、允许取消并按序展示结构化日志", async () => {
    const user = userEvent.setup();
    let finish!: (value: api.AiWorkFlowInstallStatus) => void;
    vi.mocked(api.aiWorkFlowInstallOrUpdate).mockImplementation(
      () => new Promise((resolve) => { finish = resolve; }),
    );
    vi.mocked(api.aiWorkFlowInstallCancel).mockResolvedValue({
      accepted: true,
      status: { ...idleStatus, state: "cancelled", stage: "install" },
    });
    vi.mocked(api.aiWorkFlowInstallLogs).mockResolvedValue([
      { sequence: 2, timestamp: "2", stage: "install", source: "stderr", message: "second" },
      { sequence: 1, timestamp: "1", stage: "npm_ci", source: "stdout", message: "first" },
    ]);
    renderWithProviders(<AiWorkFlowTool />);

    const install = await screen.findByRole("button", { name: /^Install$|^安装$/ });
    await user.click(install);
    expect(install).toBeDisabled();
    expect(api.aiWorkFlowInstallOrUpdate).toHaveBeenCalledOnce();
    await user.click(screen.getByRole("button", { name: /^Cancel$|^取消$/ }));
    expect(api.aiWorkFlowInstallCancel).toHaveBeenCalledOnce();
    finish({ ...idleStatus, state: "succeeded", operation: "install", stage: "complete", version: "1.2.3" });

    await waitFor(() => expect(screen.getByText("first")).toBeInTheDocument());
    expect(
      screen.getByText("first").compareDocumentPosition(screen.getByText("second")) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
  });

  it("完整读取和编辑环境 JSON，保存失败时保留文本和错误代码", async () => {
    const user = userEvent.setup();
    vi.mocked(api.aiWorkFlowEnvironmentList).mockResolvedValue([{ name: "team", current: true, valid: true }]);
    vi.mocked(api.aiWorkFlowEnvironmentStatus).mockResolvedValue({ current: "team", exists: true, valid: true });
    vi.mocked(api.aiWorkFlowEnvironmentRead).mockResolvedValue(teamDocument);
    vi.mocked(api.aiWorkFlowEnvironmentUpdate).mockRejectedValue(
      new api.AiWorkFlowError("invalid_environment_json", "Invalid JSON"),
    );
    renderWithProviders(<AiWorkFlowTool />);

    const editor = await screen.findByRole("textbox", { name: /Complete environment JSON|完整环境 JSON/ });
    expect(editor).toHaveValue(teamDocument.content);
    const invalid = "{\n  \"custom\":\n}";
    fireEvent.change(editor, { target: { value: invalid } });
    await user.click(screen.getByRole("button", { name: /^Save$|^保存$/ }));

    await waitFor(() => expect(screen.getByText("invalid_environment_json")).toBeInTheDocument());
    expect(editor).toHaveValue(invalid);
    expect(api.aiWorkFlowEnvironmentUpdate).toHaveBeenCalledWith("team", invalid);
  });

  it("支持创建、切换与删除当前环境并回退 default", async () => {
    const user = userEvent.setup();
    vi.mocked(api.aiWorkFlowEnvironmentCreate).mockResolvedValue(teamDocument);
    vi.mocked(api.aiWorkFlowEnvironmentUse).mockResolvedValue({ current: "team", exists: true, valid: true });
    vi.mocked(api.aiWorkFlowEnvironmentDelete).mockResolvedValue({ current: "default", exists: true, valid: true });
    vi.mocked(api.aiWorkFlowEnvironmentList)
      .mockResolvedValueOnce([])
      .mockResolvedValueOnce([{ name: "team", current: false, valid: true }])
      .mockResolvedValueOnce([{ name: "team", current: true, valid: true }])
      .mockResolvedValueOnce([]);
    vi.mocked(api.aiWorkFlowEnvironmentStatus)
      .mockResolvedValueOnce({ current: "default", exists: true, valid: true })
      .mockResolvedValueOnce({ current: "default", exists: true, valid: true })
      .mockResolvedValueOnce({ current: "team", exists: true, valid: true })
      .mockResolvedValueOnce({ current: "default", exists: true, valid: true });
    vi.mocked(api.aiWorkFlowEnvironmentRead).mockResolvedValue(teamDocument);
    renderWithProviders(<AiWorkFlowTool />);

    await user.type(await screen.findByRole("textbox", { name: /New environment name|新环境名称/ }), "team");
    await user.click(screen.getByRole("button", { name: /^Create$|^创建$/ }));
    await waitFor(() => expect(api.aiWorkFlowEnvironmentCreate).toHaveBeenCalledWith("team", "{\n}\n"));
    await user.click(await screen.findByRole("button", { name: /^Use$|^使用$/ }));
    await waitFor(() => expect(api.aiWorkFlowEnvironmentUse).toHaveBeenCalledWith("team"));
    await user.click(screen.getByRole("button", { name: /Delete environment|删除环境/ }));
    await waitFor(() => expect(api.aiWorkFlowEnvironmentDelete).toHaveBeenCalledWith("team"));
    expect(screen.getByText("default")).toBeInTheDocument();
  });

  it("不触发 OneSpace AI Environments 刷新事件", async () => {
    const dispatch = vi.spyOn(window, "dispatchEvent");
    renderWithProviders(<AiWorkFlowTool />);
    await screen.findByText(/No saved environments|还没有已保存环境/);
    expect(dispatch).not.toHaveBeenCalled();
    dispatch.mockRestore();
  });
});
