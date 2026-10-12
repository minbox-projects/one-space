import { describe, expect, it, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { RuntimeStatusCard } from "./RuntimeStatusCard";
import type { GatewayConfig, GatewayStatus } from "@/lib/aiGateway";

function makeConfig(overrides: Partial<GatewayConfig> = {}): GatewayConfig {
  return {
    enabled: true,
    port: 17688,
    providers: [],
    keys: [],
    default_key_id: null,
    terminal_syncs: [],
    ...overrides,
  };
}

function makeStatus(overrides: Partial<GatewayStatus> = {}): GatewayStatus {
  return {
    running: true,
    enabled: true,
    port: 17688,
    local_base_url: "http://127.0.0.1:17688/v1",
    provider_count: 0,
    auto_disabled_count: 0,
    key_count: 0,
    default_key_id: null,
    ...overrides,
  };
}

const mockConfig = makeConfig();
const mockStatusRunning = makeStatus({ running: true, provider_count: 3 });
const mockStatusStopped = makeStatus({ running: false });

describe("RuntimeStatusCard 运行时状态卡片", () => {
  it("服务运行中时展示运行状态指示灯、操作控制按钮与协议接口地址栏目，不展示端口与默认Key", () => {
    const configWithKey = makeConfig({
      keys: [{ id: "k1", label: "Main Key", value: "sk-test-key-12345", enabled: true, created_at: 1 }],
      default_key_id: "k1",
    });

    render(
      <RuntimeStatusCard
        status={mockStatusRunning}
        config={configWithKey}
        busy={false}
        addressCopied={false}
        anthropicAddressCopied={false}
        onStart={vi.fn()}
        onStop={vi.fn()}
        onCopyAddress={vi.fn()}
        onCopyAnthropicAddress={vi.fn()}
      />,
    );

    // 运行状态指示
    expect(screen.getByTestId("ai-gateway-runtime-state")).toHaveTextContent(
      /Running|运行中/,
    );
    // 不再展示端口与默认 Key 快速复制栏
    expect(screen.queryByText(/Port:|端口:/)).not.toBeInTheDocument();
    expect(screen.queryByTestId("ai-gateway-default-key-preview")).not.toBeInTheDocument();
    expect(screen.queryByTestId("ai-gateway-copy-default-key")).not.toBeInTheDocument();

    // 启停与控制按钮保留在同一栏目
    expect(screen.getByTestId("ai-gateway-toggle-service")).toBeInTheDocument();

    // 协议接口地址栏目与两个协议卡片
    expect(screen.getByTestId("ai-gateway-endpoints-section")).toBeInTheDocument();
    expect(screen.getByLabelText(/API Endpoints|协议接口地址/)).toBeInTheDocument();

    expect(screen.getByTestId("ai-gateway-local-address")).toHaveTextContent(
      "http://127.0.0.1:17688/v1",
    );
    expect(screen.getByText("OpenAI")).toBeInTheDocument();

    expect(screen.getByTestId("ai-gateway-anthropic-address")).toHaveTextContent(
      "http://127.0.0.1:17688/v1/messages",
    );
    expect(screen.getByText("Anthropic Messages")).toBeInTheDocument();
  });

  it("服务停止时展示停止状态，地址代码样式降级", () => {
    render(
      <RuntimeStatusCard
        status={mockStatusStopped}
        config={mockConfig}
        busy={false}
        addressCopied={false}
        anthropicAddressCopied={false}
        onStart={vi.fn()}
        onStop={vi.fn()}
        onCopyAddress={vi.fn()}
        onCopyAnthropicAddress={vi.fn()}
      />,
    );

    expect(screen.getByTestId("ai-gateway-runtime-state")).toHaveTextContent(
      /Stopped|已停止/,
    );
  });

  it("点击 OpenAI 与 Anthropic Messages 复制按钮分别触发对应回调", () => {
    const handleCopyOpenAi = vi.fn();
    const handleCopyAnthropic = vi.fn();

    render(
      <RuntimeStatusCard
        status={mockStatusRunning}
        config={mockConfig}
        busy={false}
        addressCopied={false}
        anthropicAddressCopied={false}
        onStart={vi.fn()}
        onStop={vi.fn()}
        onCopyAddress={handleCopyOpenAi}
        onCopyAnthropicAddress={handleCopyAnthropic}
      />,
    );

    fireEvent.click(
      screen.getByRole("button", {
        name: /Copy local API address|复制本地 Api 地址/i,
      }),
    );
    expect(handleCopyOpenAi).toHaveBeenCalledTimes(1);

    fireEvent.click(screen.getByTestId("ai-gateway-copy-anthropic-address"));
    expect(handleCopyAnthropic).toHaveBeenCalledTimes(1);
  });

  it("复制状态变为 true 时展示打勾图标反馈", () => {
    const { rerender } = render(
      <RuntimeStatusCard
        status={mockStatusRunning}
        config={mockConfig}
        busy={false}
        addressCopied={true}
        anthropicAddressCopied={false}
        onStart={vi.fn()}
        onStop={vi.fn()}
        onCopyAddress={vi.fn()}
        onCopyAnthropicAddress={vi.fn()}
      />,
    );

    expect(
      screen.getByRole("button", {
        name: /Local API address copied|本地 API 地址已复制/i,
      }),
    ).toBeInTheDocument();

    rerender(
      <RuntimeStatusCard
        status={mockStatusRunning}
        config={mockConfig}
        busy={false}
        addressCopied={false}
        anthropicAddressCopied={true}
        onStart={vi.fn()}
        onStop={vi.fn()}
        onCopyAddress={vi.fn()}
        onCopyAnthropicAddress={vi.fn()}
      />,
    );

    expect(
      screen.getByTitle(
        /Anthropic Messages API address copied|Anthropic Messages 接口地址已复制/i,
      ),
    ).toBeInTheDocument();
  });
});
