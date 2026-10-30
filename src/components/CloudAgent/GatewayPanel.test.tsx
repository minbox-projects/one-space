import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { GatewayPanel } from "./components/GatewayPanel";
import type { CloudAgentStatus } from "@/lib/cloudAgent";

const dummyT = ((key: string, defaultOrOptions?: unknown) => {
  if (typeof defaultOrOptions === "string") return defaultOrOptions;
  if (typeof defaultOrOptions === "object" && defaultOrOptions !== null && "defaultValue" in defaultOrOptions) {
    return (defaultOrOptions as { defaultValue: string }).defaultValue;
  }
  return key;
}) as any;

describe("GatewayPanel Component", () => {
  const mockStatus: CloudAgentStatus = {
    service_active: true,
    disk: {
      total_gb: 100,
      used_gb: 20,
      free_gb: 80,
      use_percent: 20,
    },
    gateway: {
      status: "healthy",
      port: 17688,
      providers_count: 2,
      model_pools: ["coder-pool"],
      providers: [
        {
          id: "deepseek-custom",
          name: "DeepSeek Provider",
          base_url: "https://api.deepseek.com",
          is_active: true,
          models: [
            {
              id: "deepseek-chat",
              upstream_model: "deepseek-chat-v3",
              display_name: "DeepSeek V3 Chat",
              context_window: 64000,
              reasoning_efforts: [],
            },
            {
              id: "deepseek-reasoner",
              upstream_model: "deepseek-r1",
              display_name: "DeepSeek R1 Reasoner",
              context_window: 128000,
              reasoning_efforts: ["low", "medium", "high"],
            },
          ],
        },
        {
          id: "anthropic-custom",
          name: "Anthropic Claude",
          base_url: "https://api.anthropic.com",
          is_active: true,
          models: [
            {
              id: "claude-3-7-sonnet",
              upstream_model: "claude-3-7-sonnet-20250219",
              display_name: "Claude 3.7 Sonnet",
              context_window: 200000,
              reasoning_efforts: ["low", "high", "max"],
            },
            {
              id: "unlimited-model",
              upstream_model: "custom-raw-model",
              display_name: null,
              context_window: null,
              reasoning_efforts: [],
            },
          ],
        },
      ],
    },
    timestamp: 1700000000,
  };

  it("renders cloud providers and models with context limits and reasoning efforts", () => {
    const onSyncGateway = vi.fn().mockResolvedValue(undefined);

    render(
      <GatewayPanel
        t={dummyT}
        status={mockStatus}
        onSyncGateway={onSyncGateway}
      />
    );

    // 验证标题与统计
    expect(screen.getByText("云端服务商与模型列表")).toBeInTheDocument();
    expect(screen.getByText("DeepSeek Provider")).toBeInTheDocument();
    expect(screen.getByText("Anthropic Claude")).toBeInTheDocument();

    // 验证模型标识与显示名称
    expect(screen.getByText("deepseek-chat")).toBeInTheDocument();
    expect(screen.getByText("DeepSeek V3 Chat")).toBeInTheDocument();
    expect(screen.getByText("deepseek-chat-v3")).toBeInTheDocument();

    expect(screen.getByText("deepseek-reasoner")).toBeInTheDocument();
    expect(screen.getByText("DeepSeek R1 Reasoner")).toBeInTheDocument();
    expect(screen.getByText("deepseek-r1")).toBeInTheDocument();

    // 验证上下文长度（64K, 128K, 200K）
    expect(screen.getByText("64K")).toBeInTheDocument();
    expect(screen.getByText("128K")).toBeInTheDocument();
    expect(screen.getByText("200K")).toBeInTheDocument();

    // 验证推理强度药丸
    expect(screen.getAllByText("low").length).toBe(2);
    expect(screen.getByText("medium")).toBeInTheDocument();
    expect(screen.getAllByText("high").length).toBe(2);
    expect(screen.getByText("max")).toBeInTheDocument();
  });

  it("toggles provider section collapse on header click", async () => {
    const user = userEvent.setup();
    render(
      <GatewayPanel
        t={dummyT}
        status={mockStatus}
        onSyncGateway={vi.fn().mockResolvedValue(undefined)}
      />
    );

    expect(screen.getByText("deepseek-chat")).toBeInTheDocument();

    // 点击 DeepSeek 服务商头部折叠
    const providerHeader = screen.getByText("DeepSeek Provider");
    await user.click(providerHeader);

    // 折叠后模型隐藏
    expect(screen.queryByText("deepseek-chat")).not.toBeInTheDocument();

    // 再次点击展开
    await user.click(providerHeader);
    expect(screen.getByText("deepseek-chat")).toBeInTheDocument();
  });

  it("filters models and providers by search query", async () => {
    const user = userEvent.setup();
    render(
      <GatewayPanel
        t={dummyT}
        status={mockStatus}
        onSyncGateway={vi.fn().mockResolvedValue(undefined)}
      />
    );

    const searchInput = screen.getByPlaceholderText("搜索服务商或模型...");
    await user.type(searchInput, "sonnet");

    // 仅显示匹配的模型
    expect(screen.getByText("claude-3-7-sonnet")).toBeInTheDocument();
    expect(screen.queryByText("deepseek-chat")).not.toBeInTheDocument();
  });

  it("handles empty provider state gracefully", () => {
    const emptyStatus: CloudAgentStatus = {
      ...mockStatus,
      gateway: {
        status: "healthy",
        port: 17688,
        providers: [],
        model_pools: [],
      },
    };

    render(
      <GatewayPanel
        t={dummyT}
        status={emptyStatus}
        onSyncGateway={vi.fn().mockResolvedValue(undefined)}
      />
    );

    expect(
      screen.getByText(
        "云端暂无服务商与模型映射，请点击右上角「一键同步网关至云端」完成配置下发。"
      )
    ).toBeInTheDocument();
  });

  it("triggers sync gateway callback on button click", async () => {
    const user = userEvent.setup();
    const onSyncGateway = vi.fn().mockResolvedValue({
      success: true,
      providers_synced: 2,
      model_pools_synced: 1,
      message: "同步成功",
    });

    render(
      <GatewayPanel
        t={dummyT}
        status={mockStatus}
        onSyncGateway={onSyncGateway}
      />
    );

    const syncBtn = screen.getByText("一键同步网关至云端");
    await user.click(syncBtn);

    expect(onSyncGateway).toHaveBeenCalledTimes(1);
    expect(await screen.findByText("同步成功")).toBeInTheDocument();
  });
});
