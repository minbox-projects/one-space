import { beforeEach, describe, expect, it } from "vitest";
import {
  remoteBotGetConfig,
  remoteBotGetStatus,
  remoteBotRestart,
  remoteBotSaveConfig,
  remoteBotTestConnection,
} from "./remoteBot";
import { invokeMock, resetTauriMocks } from "@/test/mocks/tauri";

describe("remoteBot IPC", () => {
  beforeEach(() => {
    resetTauriMocks();
  });

  it("remoteBotGetConfig 调用 remote_bot_get_config", async () => {
    const mockConfig = {
      enabled: true,
      bot_token: "test_token",
      allowed_chat_ids: [123456],
      active_workspace: "onespace-app",
      auto_approve_permissions: true,
      updated_at: 1700000000,
    };
    invokeMock.mockResolvedValueOnce(mockConfig);

    const res = await remoteBotGetConfig();
    expect(res).toEqual(mockConfig);
    expect(invokeMock).toHaveBeenCalledWith("remote_bot_get_config");
  });

  it("remoteBotSaveConfig 调用 remote_bot_save_config", async () => {
    const mockConfig = {
      enabled: true,
      bot_token: "test_token",
      allowed_chat_ids: [123456],
      active_workspace: "onespace-app",
      auto_approve_permissions: true,
      updated_at: 1700000000,
    };
    invokeMock.mockResolvedValueOnce(undefined);

    await remoteBotSaveConfig(mockConfig);
    expect(invokeMock).toHaveBeenCalledWith("remote_bot_save_config", { config: mockConfig });
  });

  it("remoteBotTestConnection 调用 remote_bot_test_connection", async () => {
    const mockUser = { id: 999, is_bot: true, first_name: "TestBot", username: "test_bot" };
    invokeMock.mockResolvedValueOnce(mockUser);

    const res = await remoteBotTestConnection("mock_token");
    expect(res).toEqual(mockUser);
    expect(invokeMock).toHaveBeenCalledWith("remote_bot_test_connection", { botToken: "mock_token" });
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
});
