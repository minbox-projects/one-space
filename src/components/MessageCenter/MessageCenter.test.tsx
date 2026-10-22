import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MessageCenter } from "./MessageCenter";
import type { MessageRecord } from "@/lib/messages";

const invokeMock = vi.fn();
const listenMock = vi.fn().mockResolvedValue(() => {});

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: (...args: unknown[]) => listenMock(...args),
}));

const mockMessages: MessageRecord[] = [
  {
    id: "msg_1",
    source: "ai_gateway",
    category: "upstream",
    severity: "error",
    title: "Claude 3.5 Sonnet upstream timeout",
    summary: "Request timed out after 5000ms",
    detail: "Error: connect timeout at dispatch (router.rs:142)",
    created_at: 1700000000,
    last_seen_at: 1700000100,
    read_at: null,
    occurrences: 3,
    target: { tab: "ai-gateway", section: "runtime" },
    metadata: { model: "claude-3-5" },
  },
  {
    id: "msg_2",
    source: "ssh_tunnels",
    category: "supervisor",
    severity: "warning",
    title: "Tunnel db-tunnel disconnected",
    summary: "Attempting reconnect in 10s",
    detail: "Connection dropped unexpectedly",
    created_at: 1700000010,
    last_seen_at: 1700000020,
    read_at: 1700000030,
    occurrences: 1,
    target: { tab: "ssh-tunnels" },
  },
  {
    id: "msg_3",
    source: "skills",
    category: "sync",
    severity: "success",
    title: "Skills synchronized",
    summary: "4 skills updated from registry",
    detail: null,
    created_at: 1700000040,
    last_seen_at: 1700000040,
    read_at: null,
    occurrences: 1,
    target: { tab: "skills" },
  },
];

describe("src/components/MessageCenter/MessageCenter.tsx", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    listenMock.mockReset().mockResolvedValue(() => {});
    localStorage.clear();
  });

  it("renders empty state when there are no messages", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "messages_list") return [];
      if (command === "messages_unread_count") return 0;
      return null;
    });

    render(
      <MessageCenter
        open={true}
        onOpenChange={vi.fn()}
        onNavigate={vi.fn()}
      />,
    );

    await waitFor(() => {
      expect(screen.getByText(/暂无消息|No messages yet/i)).toBeInTheDocument();
    });
  });

  it("renders messages list and computes unread count correctly", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "messages_list") return mockMessages;
      if (command === "messages_unread_count") return 2;
      return null;
    });

    render(
      <MessageCenter
        open={true}
        onOpenChange={vi.fn()}
        onNavigate={vi.fn()}
      />,
    );

    await waitFor(() => {
      expect(
        screen.getByText("Claude 3.5 Sonnet upstream timeout"),
      ).toBeInTheDocument();
      expect(
        screen.getByText("Tunnel db-tunnel disconnected"),
      ).toBeInTheDocument();
      expect(screen.getByText("Skills synchronized")).toBeInTheDocument();
    });

    // 两个未读消息
    expect(screen.getByText(/2 unread|2 条未读/i)).toBeInTheDocument();
  });

  it("filters messages by Unread and Alerts tabs", async () => {
    const user = userEvent.setup();
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "messages_list") return mockMessages;
      return null;
    });

    render(
      <MessageCenter
        open={true}
        onOpenChange={vi.fn()}
        onNavigate={vi.fn()}
      />,
    );

    await waitFor(() => {
      expect(screen.getByText("Claude 3.5 Sonnet upstream timeout")).toBeInTheDocument();
    });

    // 点击未读 Tab
    const unreadTab = screen.getByRole("button", { name: /^未读|^Unread/i });
    await user.click(unreadTab);

    // msg_1 (未读) 和 msg_3 (未读) 显示，msg_2 (已读) 隐藏
    expect(screen.getByText("Claude 3.5 Sonnet upstream timeout")).toBeInTheDocument();
    expect(screen.getByText("Skills synchronized")).toBeInTheDocument();
    expect(screen.queryByText("Tunnel db-tunnel disconnected")).not.toBeInTheDocument();

    // 点击告警 Tab
    const alertsTab = screen.getByRole("button", { name: /^告警与错误|^Alerts/i });
    await user.click(alertsTab);

    // msg_1 (error) 和 msg_2 (warning) 显示，msg_3 (success) 隐藏
    expect(screen.getByText("Claude 3.5 Sonnet upstream timeout")).toBeInTheDocument();
    expect(screen.getByText("Tunnel db-tunnel disconnected")).toBeInTheDocument();
    expect(screen.queryByText("Skills synchronized")).not.toBeInTheDocument();
  });

  it("searches messages by query and handles clear filters", async () => {
    const user = userEvent.setup();
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "messages_list") return mockMessages;
      return null;
    });

    render(
      <MessageCenter
        open={true}
        onOpenChange={vi.fn()}
        onNavigate={vi.fn()}
      />,
    );

    await waitFor(() => {
      expect(screen.getByText("Claude 3.5 Sonnet upstream timeout")).toBeInTheDocument();
    });

    const searchInput = screen.getByPlaceholderText(/搜索|Search/i);
    await user.type(searchInput, "router.rs");

    // 仅命中带堆栈的 msg_1
    expect(screen.getByText("Claude 3.5 Sonnet upstream timeout")).toBeInTheDocument();
    expect(screen.queryByText("Tunnel db-tunnel disconnected")).not.toBeInTheDocument();
    expect(screen.queryByText("Skills synchronized")).not.toBeInTheDocument();

    // 搜索不存在的关键词
    await user.clear(searchInput);
    await user.type(searchInput, "nonexistent-error-phrase");
    expect(screen.getByText(/没有找到匹配的消息|No matching messages/i)).toBeInTheDocument();

    // 点击清空筛选按钮
    const clearBtn = screen.getAllByRole("button", { name: /清空筛选|Clear filters/i })[0];
    await user.click(clearBtn);

    // 恢复所有消息展示
    expect(screen.getByText("Claude 3.5 Sonnet upstream timeout")).toBeInTheDocument();
    expect(screen.getByText("Tunnel db-tunnel disconnected")).toBeInTheDocument();
  });

  it("opens detail view on message click and navigates back", async () => {
    const user = userEvent.setup();
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "messages_list") return mockMessages;
      if (command === "messages_mark_read") return null;
      return null;
    });

    render(
      <MessageCenter
        open={true}
        onOpenChange={vi.fn()}
        onNavigate={vi.fn()}
      />,
    );

    await waitFor(() => {
      expect(screen.getByText("Claude 3.5 Sonnet upstream timeout")).toBeInTheDocument();
    });

    // 点击第一条消息卡片
    await user.click(screen.getByText("Claude 3.5 Sonnet upstream timeout"));

    // 详情滑入展示
    await waitFor(() => {
      expect(screen.getByText("Error: connect timeout at dispatch (router.rs:142)")).toBeInTheDocument();
      expect(screen.getByText(/返回列表|Back to list/i)).toBeInTheDocument();
    });

    // 触发了标为已读
    expect(invokeMock).toHaveBeenCalledWith("messages_mark_read", { id: "msg_1" });

    // 点击返回列表
    await user.click(screen.getByRole("button", { name: /返回列表|Back to list/i }));
    await waitFor(() => {
      expect(screen.getByText("Skills synchronized")).toBeInTheDocument();
    });
  });

  it("handles navigation to related feature", async () => {
    const user = userEvent.setup();
    const onNavigate = vi.fn();
    const onOpenChange = vi.fn();

    invokeMock.mockImplementation(async (command: string) => {
      if (command === "messages_list") return mockMessages;
      if (command === "messages_mark_read") return null;
      return null;
    });

    render(
      <MessageCenter
        open={true}
        onOpenChange={onOpenChange}
        onNavigate={onNavigate}
      />,
    );

    await waitFor(() => {
      expect(screen.getByText("Claude 3.5 Sonnet upstream timeout")).toBeInTheDocument();
    });

    await user.click(screen.getByText("Claude 3.5 Sonnet upstream timeout"));

    const navBtn = await screen.findByRole("button", {
      name: /打开相关功能|Open related feature/i,
    });
    await user.click(navBtn);

    expect(onOpenChange).toHaveBeenCalledWith(false);
    expect(onNavigate).toHaveBeenCalledWith({
      tab: "ai-gateway",
      section: "runtime",
    });
  });

  it("toggles wide mode split view", async () => {
    const user = userEvent.setup();
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "messages_list") return mockMessages;
      return null;
    });

    render(
      <MessageCenter
        open={true}
        onOpenChange={vi.fn()}
        onNavigate={vi.fn()}
      />,
    );

    await waitFor(() => {
      expect(screen.getByText("Claude 3.5 Sonnet upstream timeout")).toBeInTheDocument();
    });

    const toggleWideBtn = screen.getByTitle(/宽屏排障工作台|Split view/i);
    await user.click(toggleWideBtn);

    // 工作台模式右侧提示
    expect(
      screen.getByText(/选择一条消息以查看详情|Select a message/i),
    ).toBeInTheDocument();
  });

  it("marks all messages as read", async () => {
    const user = userEvent.setup();
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "messages_list") return mockMessages;
      if (command === "messages_mark_all_read") return null;
      return null;
    });

    render(
      <MessageCenter
        open={true}
        onOpenChange={vi.fn()}
        onNavigate={vi.fn()}
      />,
    );

    await waitFor(() => {
      expect(screen.getByText("Claude 3.5 Sonnet upstream timeout")).toBeInTheDocument();
    });

    const markAllBtn = screen.getByTitle(/全部标为已读|Mark all as read/i);
    await user.click(markAllBtn);

    expect(invokeMock).toHaveBeenCalledWith("messages_mark_all_read");
    await waitFor(() => {
      expect(screen.getByText(/暂无未读消息|All caught up/i)).toBeInTheDocument();
    });
  });

  it("closes on Escape key press", async () => {
    const onOpenChange = vi.fn();
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "messages_list") return mockMessages;
      return null;
    });

    render(
      <MessageCenter
        open={true}
        onOpenChange={onOpenChange}
        onNavigate={vi.fn()}
      />,
    );

    await waitFor(() => {
      expect(screen.getByText("Claude 3.5 Sonnet upstream timeout")).toBeInTheDocument();
    });

    fireEvent.keyDown(window, { key: "Escape" });
    expect(onOpenChange).toHaveBeenCalledWith(false);
  });
});
