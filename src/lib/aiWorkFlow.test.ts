import { invoke } from "@tauri-apps/api/core";
import { describe, expect, it, vi } from "vitest";
import {
  AiWorkFlowError,
  aiWorkFlowEnvironmentCreate,
  aiWorkFlowEnvironmentDelete,
  aiWorkFlowEnvironmentList,
  aiWorkFlowEnvironmentRead,
  aiWorkFlowEnvironmentStatus,
  aiWorkFlowEnvironmentUpdate,
  aiWorkFlowEnvironmentUse,
  aiWorkFlowInstallCancel,
  aiWorkFlowInstallLogs,
  aiWorkFlowInstallOrUpdate,
  aiWorkFlowInstallStatus,
  aiWorkFlowInstallVersion,
} from "@/lib/aiWorkFlow";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

describe("AI Work Flow invoke 契约", () => {
  it("逐一调用后端注册的固定命令并只传环境名称与完整内容", async () => {
    vi.mocked(invoke).mockResolvedValue({});

    await aiWorkFlowInstallStatus();
    await aiWorkFlowInstallVersion();
    await aiWorkFlowInstallOrUpdate();
    await aiWorkFlowInstallCancel();
    await aiWorkFlowInstallLogs();
    await aiWorkFlowEnvironmentList();
    await aiWorkFlowEnvironmentCreate("team", "{\n  \"version\": 1,\n  \"roles\": {}\n}\n");
    await aiWorkFlowEnvironmentRead("team");
    await aiWorkFlowEnvironmentUpdate("team", "{\n  \"version\": 1,\n  \"roles\": {}\n}\n");
    await aiWorkFlowEnvironmentDelete("team");
    await aiWorkFlowEnvironmentUse("team");
    await aiWorkFlowEnvironmentStatus();

    expect(vi.mocked(invoke).mock.calls).toEqual([
      ["ai_work_flow_install_status_get"],
      ["ai_work_flow_install_version_get"],
      ["ai_work_flow_install_or_update"],
      ["ai_work_flow_install_cancel"],
      ["ai_work_flow_install_logs_get"],
      ["ai_work_flow_environment_list"],
      ["ai_work_flow_environment_create", { name: "team", content: "{\n  \"version\": 1,\n  \"roles\": {}\n}\n" }],
      ["ai_work_flow_environment_read", { name: "team" }],
      ["ai_work_flow_environment_update", { name: "team", content: "{\n  \"version\": 1,\n  \"roles\": {}\n}\n" }],
      ["ai_work_flow_environment_delete", { name: "team" }],
      ["ai_work_flow_environment_use", { name: "team" }],
      ["ai_work_flow_environment_status"],
    ]);
  });

  it("保留后端稳定错误代码与消息", async () => {
    vi.mocked(invoke).mockRejectedValueOnce({
      code: "invalid_environment_json",
      message: "Environment JSON is invalid",
    });

    await expect(aiWorkFlowEnvironmentUpdate("broken", "{")).rejects.toEqual(
      expect.objectContaining<Partial<AiWorkFlowError>>({
        code: "invalid_environment_json",
        message: "Environment JSON is invalid",
      }),
    );
  });
});
