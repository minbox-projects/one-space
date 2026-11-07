import { beforeEach, describe, expect, it } from "vitest";
import {
  remoteBotGetConfig,
  remoteBotGetStatus,
  remoteBotRestart,
  remoteBotSaveConfig,
  remoteBotTestConnection,
  remoteBotProbeOpencode,
  remoteBotListInstalledOpencode,
  type RemoteBotConfig,
  type OpenCodeInstalledVersion,
} from "./remoteBot";
import { invokeMock, resetTauriMocks } from "@/test/mocks/tauri";

// 移除明文 Token，改为从环境变量 TELEGRAM_BOT_TOKEN 中安全读取
const TEST_BOT_TOKEN = process.env.TELEGRAM_BOT_TOKEN || "env_test_token";

describe("remoteBot IPC", () => {
  beforeEach(() => {
    resetTauriMocks();
  });

  const baseConfig: RemoteBotConfig = {
    enabled: true,
    bot_token: TEST_BOT_TOKEN,
    allowed_chat_ids: [123456],
    active_workspace: "onespace-app",
    auto_approve_permissions: true,
    enable_natural_dialogue: true,
    opencode_bin_path: null,
    workspace_directories: ["/test/workspace"],
    updated_at: 1700000000,
  };

  it("remoteBotGetConfig 调用 remote_bot_get_config", async () => {
    invokeMock.mockResolvedValueOnce(baseConfig);

    const res = await remoteBotGetConfig();
    expect(res).toEqual(baseConfig);
    expect(invokeMock).toHaveBeenCalledWith("remote_bot_get_config");
  });

  it("remoteBotSaveConfig 调用 remote_bot_save_config", async () => {
    invokeMock.mockResolvedValueOnce(undefined);

    await remoteBotSaveConfig(baseConfig);
    expect(invokeMock).toHaveBeenCalledWith("remote_bot_save_config", { config: baseConfig });
  });

  it("remoteBotTestConnection 调用 remote_bot_test_connection", async () => {
    const mockUser = { id: 999, is_bot: true, first_name: "TestBot", username: "test_bot" };
    invokeMock.mockResolvedValueOnce(mockUser);

    const res = await remoteBotTestConnection(TEST_BOT_TOKEN);
    expect(res).toEqual(mockUser);
    expect(invokeMock).toHaveBeenCalledWith("remote_bot_test_connection", { botToken: TEST_BOT_TOKEN });
  });

  it("remoteBotGetStatus 调用 remote_bot_get_status", async () => {
    const mockStatus = {
      running: true,
      bot_username: "test_bot",
      bot_name: "TestBot",
      last_poll_time: 1700000000,
      last_error: null,
      active_task_id: null,
      pending_question_id: null,
    };
    invokeMock.mockResolvedValueOnce(mockStatus);

    const res = await remoteBotGetStatus();
    expect(res).toEqual(mockStatus);
    expect(invokeMock).toHaveBeenCalledWith("remote_bot_get_status");
  });

  it("remoteBotRestart 调用 remote_bot_restart", async () => {
    const mockStatus = {
      running: true,
      bot_username: "test_bot",
      bot_name: "TestBot",
      last_poll_time: 1700000000,
      last_error: null,
      active_task_id: null,
      pending_question_id: null,
    };
    invokeMock.mockResolvedValueOnce(mockStatus);

    const res = await remoteBotRestart();
    expect(res).toEqual(mockStatus);
    expect(invokeMock).toHaveBeenCalledWith("remote_bot_restart");
  });

  it("remoteBotProbeOpencode 调用 remote_bot_probe_opencode", async () => {
    invokeMock.mockResolvedValueOnce("opencode 1.0.0");

    const res = await remoteBotProbeOpencode("~/.local/bin/opencode");
    expect(res).toBe("opencode 1.0.0");
    expect(invokeMock).toHaveBeenCalledWith("remote_bot_probe_opencode", {
      binPath: "~/.local/bin/opencode",
    });
  });

  it("remoteBotGetRunningOpencodeSessions 调用 remote_bot_get_running_opencode_sessions", async () => {
    const mockSessions = [
      {
        pid: 12345,
        ppid: 100,
        elapsed: "10:00",
        command: "opencode --yolo",
        cwd: "/path/to/project",
        project_name: "project",
        is_busy: true,
        todos: [],
        tokens_input: 1000,
        tokens_output: 500,
        tokens_reasoning: 200,
        tokens_cache_read: 5000,
      },
    ];
    invokeMock.mockResolvedValueOnce(mockSessions);

    const res = await (await import("./remoteBot")).remoteBotGetRunningOpencodeSessions();
    expect(res).toEqual(mockSessions);
    expect(invokeMock).toHaveBeenCalledWith("remote_bot_get_running_opencode_sessions");
  });

  it("remoteBotGetHostedSessions 调用 remote_bot_get_hosted_sessions", async () => {
    const mockHosted = [
      {
        session_id: "ses_test_123",
        pid: 12345,
        cwd: "/path/to/project",
        project_name: "project",
        chat_id: 123456,
        mode: "Manual" as const,
        started_at: 1700000000,
        last_observed_step: 5,
        completed_todos_count: 2,
        total_todos_count: 4,
      },
    ];
    invokeMock.mockResolvedValueOnce(mockHosted);

    const res = await (await import("./remoteBot")).remoteBotGetHostedSessions();
    expect(res).toEqual(mockHosted);
    expect(invokeMock).toHaveBeenCalledWith("remote_bot_get_hosted_sessions");
  });

  it("remoteBotListInstalledOpencode 调用 remote_bot_list_installed_opencode", async () => {
    const mockVersions: OpenCodeInstalledVersion[] = [
      {
        path: "/usr/local/bin/opencode",
        version: "2.0.4",
        major_version: 2,
        version_type: "v2",
        is_active: true,
      },
      {
        path: "/Users/test/.opencode/bin/opencode",
        version: "1.19.5",
        major_version: 1,
        version_type: "v1",
        is_active: false,
      },
    ];
    invokeMock.mockResolvedValueOnce(mockVersions);

    const res = await remoteBotListInstalledOpencode();
    expect(res).toEqual(mockVersions);
    expect(invokeMock).toHaveBeenCalledWith("remote_bot_list_installed_opencode");
  });
});
