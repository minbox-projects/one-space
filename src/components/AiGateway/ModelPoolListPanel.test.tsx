import { fireEvent, screen, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import i18n from "@/i18n";
import { renderWithProviders } from "@/test/mocks/render";
import { ModelPoolListPanel } from "./ModelPoolListPanel";
import type {
  GatewayModelPool,
  GatewayModelPoolStatus,
  GatewayUpstreamProvider,
} from "@/lib/aiGateway";

const mockProviders: GatewayUpstreamProvider[] = [
  {
    id: "p-a",
    name: "Provider A",
    base_url: "https://api.a.example",
    default_model: null,
    protocol: "chat_completions",
    mappings: [{ local_model: "claude-sonnet", upstream_model: "claude-3-5-sonnet" }],
    enabled: true,
  },
];

const mockPools: GatewayModelPool[] = [
  {
    id: "pool-1",
    name: "Claude High Availability",
    model: "anthropic/claude-sonnet-pool",
    enabled: true,
    candidates: [
      {
        provider_id: "p-a",
        local_model: "claude-sonnet",
        upstream_model: "claude-3-5-sonnet",
        protocol: "chat_completions",
        weight: 1,
      },
    ],
  },
  {
    id: "pool-2",
    name: "Disabled Backup Pool",
    model: "anthropic/claude-backup-pool",
    enabled: false,
    candidates: [
      {
        provider_id: "p-a",
        local_model: "claude-sonnet",
        upstream_model: "claude-3-5-sonnet",
        protocol: "chat_completions",
        weight: 1,
      },
    ],
  },
];

const mockStatuses: GatewayModelPoolStatus[] = [
  {
    id: "pool-1",
    name: "Claude High Availability",
    model: "anthropic/claude-sonnet-pool",
    enabled: true,
    candidates: [],
  },
];

describe("ModelPoolListPanel", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("en");
  });

  it("renders empty state guide when no model pools exist and triggers onAddPool", () => {
    const onAddPool = vi.fn();
    renderWithProviders(
      <ModelPoolListPanel
        modelPools={[]}
        providers={mockProviders}
        onAddPool={onAddPool}
      />,
    );

    expect(screen.getByTestId("ai-gateway-model-pools-panel")).toBeInTheDocument();
    expect(screen.getByTestId("ai-gateway-model-pools-empty")).toBeInTheDocument();
    expect(screen.queryByTestId("ai-gateway-model-pools-toolbar")).not.toBeInTheDocument();

    const addButtons = screen.getAllByRole("button", { name: /add model pool/i });
    expect(addButtons.length).toBeGreaterThan(0);
    fireEvent.click(addButtons[0]);
    expect(onAddPool).toHaveBeenCalledTimes(1);
  });

  it("renders header, create button, and filter toolbar when model pools exist", () => {
    const onAddPool = vi.fn();
    renderWithProviders(
      <ModelPoolListPanel
        modelPools={mockPools}
        providers={mockProviders}
        poolStatuses={mockStatuses}
        onAddPool={onAddPool}
      />,
    );

    const panel = screen.getByTestId("ai-gateway-model-pools-panel");
    expect(within(panel).getByText("Model pools")).toBeInTheDocument();
    expect(screen.getByTestId("ai-gateway-model-pools-count")).toHaveTextContent("2");

    const headerAddBtn = screen.getByTestId("ai-gateway-add-model-pool");
    expect(headerAddBtn).toBeInTheDocument();
    fireEvent.click(headerAddBtn);
    expect(onAddPool).toHaveBeenCalledTimes(1);

    expect(screen.getByTestId("ai-gateway-model-pools-toolbar")).toBeInTheDocument();
    expect(screen.getByTestId("filter-pool-status-all")).toBeInTheDocument();
    expect(screen.getByTestId("filter-pool-status-enabled")).toBeInTheDocument();
    expect(screen.getByTestId("filter-pool-status-disabled")).toBeInTheDocument();
  });

  it("filters model pools by status", () => {
    renderWithProviders(
      <ModelPoolListPanel
        modelPools={mockPools}
        providers={mockProviders}
        poolStatuses={mockStatuses}
      />,
    );

    // Initial: all 2 pools shown
    expect(screen.getByText("Claude High Availability")).toBeInTheDocument();
    expect(screen.getByText("Disabled Backup Pool")).toBeInTheDocument();

    // Filter enabled
    fireEvent.click(screen.getByTestId("filter-pool-status-enabled"));
    expect(screen.getByText("Claude High Availability")).toBeInTheDocument();
    expect(screen.queryByText("Disabled Backup Pool")).not.toBeInTheDocument();

    // Filter disabled
    fireEvent.click(screen.getByTestId("filter-pool-status-disabled"));
    expect(screen.queryByText("Claude High Availability")).not.toBeInTheDocument();
    expect(screen.getByText("Disabled Backup Pool")).toBeInTheDocument();

    // Reset to all
    fireEvent.click(screen.getByTestId("filter-pool-status-all"));
    expect(screen.getByText("Claude High Availability")).toBeInTheDocument();
    expect(screen.getByText("Disabled Backup Pool")).toBeInTheDocument();
  });

  it("renders filter empty state and allows resetting when no pools match", () => {
    const onlyEnabledPools = [mockPools[0]];
    renderWithProviders(
      <ModelPoolListPanel
        modelPools={onlyEnabledPools}
        providers={mockProviders}
      />,
    );

    // Click disabled filter
    fireEvent.click(screen.getByTestId("filter-pool-status-disabled"));
    expect(screen.getByTestId("ai-gateway-model-pools-filter-empty")).toBeInTheDocument();
    expect(screen.queryByText("Claude High Availability")).not.toBeInTheDocument();

    // Click clear filter / show all
    const showAllBtn = within(
      screen.getByTestId("ai-gateway-model-pools-filter-empty"),
    ).getByRole("button");
    fireEvent.click(showAllBtn);

    expect(screen.getByText("Claude High Availability")).toBeInTheDocument();
  });

  it("propagates toggle enabled, edit, delete, and open logs callbacks", () => {
    const onTogglePoolEnabled = vi.fn();
    const onEditPool = vi.fn();
    const onDeletePool = vi.fn();
    const onOpenPoolLogs = vi.fn();

    renderWithProviders(
      <ModelPoolListPanel
        modelPools={mockPools}
        providers={mockProviders}
        poolStatuses={mockStatuses}
        onTogglePoolEnabled={onTogglePoolEnabled}
        onEditPool={onEditPool}
        onDeletePool={onDeletePool}
        onOpenPoolLogs={onOpenPoolLogs}
      />,
    );

    // Toggle switch
    const enableSwitch = screen.getByTestId("ai-gateway-model-pool-enable-pool-1");
    fireEvent.click(enableSwitch);
    expect(onTogglePoolEnabled).toHaveBeenCalledWith("pool-1", false);

    // Edit button
    const editBtn = screen.getByTestId("ai-gateway-model-pool-edit-pool-1");
    fireEvent.click(editBtn);
    expect(onEditPool).toHaveBeenCalledWith(mockPools[0]);

    // Open logs
    const logBtn = screen.getByTestId("ai-gateway-model-pool-log-pool-1");
    fireEvent.click(logBtn);
    expect(onOpenPoolLogs).toHaveBeenCalledWith("anthropic/claude-sonnet-pool");
  });
});

// ---------------------------------------------------------------------------
// Step 4 (20261015-pool-only-reasoning) REQ-005 / AC-009 (UI): the pool panel
// forwards each pool's status so its card can advertise the configured
// canonical support/coverage; a pool without any status keeps no reasoning
// block. Nothing is inferred inside the panel.
// ---------------------------------------------------------------------------

describe("ModelPoolListPanel 模型池推理能力透传（Step 4）", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("en");
  });

  const reasoningPool = {
    id: "pool-reasoning",
    name: "Reasoning Pool",
    model: "reasoning-pool",
    enabled: true,
    candidates: [
      {
        provider_id: "p-a",
        local_model: "claude-sonnet",
        upstream_model: "claude-3-5-sonnet",
        protocol: "chat_completions",
        weight: 1,
      },
    ],
  };

  const reasoningStatus = {
    id: "pool-reasoning",
    name: "Reasoning Pool",
    model: "reasoning-pool",
    enabled: true,
    default_reasoning_effort: "high",
    reasoning_efforts: ["high"],
    reasoning_coverage: [
      { effort: "high", mapped_candidates: 2, total_candidates: 3 },
    ],
    candidates: [],
  };

  it("把状态透传给卡片展示配置支持与静态覆盖", () => {
    renderWithProviders(
      <ModelPoolListPanel
        modelPools={[reasoningPool as unknown as GatewayModelPool]}
        providers={mockProviders}
        poolStatuses={[reasoningStatus as unknown as GatewayModelPoolStatus]}
      />,
    );

    expect(
      screen.getByTestId(
        "ai-gateway-model-pool-reasoning-support-pool-reasoning",
      ),
    ).toHaveTextContent("high");
    expect(
      screen.getByTestId(
        "ai-gateway-model-pool-reasoning-coverage-pool-reasoning",
      ),
    ).toHaveTextContent(
      i18n.t("aiGatewayModelPoolReasoningCoverageValue", {
        mapped: 2,
        total: 3,
      }),
    );
  });

  it("没有状态数据的池不展示推理块", () => {
    renderWithProviders(
      <ModelPoolListPanel
        modelPools={[reasoningPool as unknown as GatewayModelPool]}
        providers={mockProviders}
      />,
    );

    expect(
      screen.queryByTestId(
        "ai-gateway-model-pool-reasoning-support-pool-reasoning",
      ),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByTestId(
        "ai-gateway-model-pool-reasoning-coverage-pool-reasoning",
      ),
    ).not.toBeInTheDocument();
  });
});

// ---------------------------------------------------------------------------
// Ordered fallback models: the panel forwards each pool's status, including the
// ordered per-model fallback status array, so the card lists every stored
// fallback gateway model next to the regular candidates.
//
// AC-006 · REQ-004.
// ---------------------------------------------------------------------------

describe("ModelPoolListPanel 有序回退状态透传（Step 3）", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("en");
  });

  const fallbackProviders = [
    {
      id: "p-a",
      name: "Provider A",
      base_url: "https://api.a.example",
      default_model: null,
      protocol: "chat_completions",
      enabled: true,
      mappings: [{ local_model: "local-a1", upstream_model: "remote-a1" }],
    },
    {
      id: "p-b",
      name: "Provider B",
      base_url: "https://api.b.example",
      default_model: null,
      protocol: "responses",
      enabled: true,
      mappings: [{ local_model: "local-b1", upstream_model: "remote-b1" }],
    },
    {
      id: "p-c",
      name: "Provider C",
      base_url: "https://api.c.example",
      default_model: null,
      protocol: "responses",
      enabled: true,
      mappings: [{ local_model: "local-c1", upstream_model: "remote-c1" }],
    },
  ] as unknown as GatewayUpstreamProvider[];

  it("把有序回退状态数组透传给卡片并按存储顺序展示", () => {
    const pool = {
      id: "pool-fallback",
      name: "Pool One",
      model: "pool-model",
      enabled: true,
      candidates: [
        {
          provider_id: "p-a",
          local_model: "local-a1",
          upstream_model: "remote-a1",
          protocol: "chat_completions",
          weight: 1,
        },
      ],
      fallback_models: ["local-b1", "local-c1"],
    } as unknown as GatewayModelPool;

    const status = {
      id: "pool-fallback",
      name: "Pool One",
      model: "pool-model",
      enabled: true,
      candidates: [
        {
          provider_id: "p-a",
          provider_name: "Provider A",
          local_model: "local-a1",
          upstream_model: "remote-a1",
          protocol: "chat_completions",
          weight: 1,
          ready: false,
          availability: "cooling",
          reason: "cooling after a model-scoped failure",
          cooling_until: 1_900_000_000,
        },
      ],
      fallback_models: [
        {
          model: "local-b1",
          ready: true,
          ready_sources: 1,
          total_sources: 1,
          sources: [
            {
              provider_id: "p-b",
              provider_name: "Provider B",
              local_model: "local-b1",
              upstream_model: "remote-b1",
              protocol: "responses",
              ready: true,
              availability: "ready",
              reason: null,
              cooling_until: null,
            },
          ],
        },
        {
          model: "local-c1",
          ready: false,
          ready_sources: 0,
          total_sources: 1,
          sources: [
            {
              provider_id: "p-c",
              provider_name: "Provider C",
              local_model: "local-c1",
              upstream_model: "remote-c1",
              protocol: "responses",
              ready: false,
              availability: "no_usable_key",
              reason: "no usable key",
              cooling_until: null,
            },
          ],
          reason: "no usable key",
        },
      ],
    } as unknown as GatewayModelPoolStatus;

    renderWithProviders(
      <ModelPoolListPanel
        modelPools={[pool]}
        providers={fallbackProviders}
        poolStatuses={[status]}
      />,
    );

    const card = screen.getByTestId("ai-gateway-model-pool-pool-fallback");
    const fallback = within(card).getByTestId(
      "ai-gateway-model-pool-fallback-pool-fallback",
    );
    const text = fallback.textContent ?? "";
    expect(text).toContain("local-b1");
    expect(text).toContain("local-c1");
    expect(text.indexOf("local-b1")).toBeLessThan(text.indexOf("local-c1"));
    expect(within(fallback).queryByText("Provider B")).not.toBeInTheDocument();
    expect(within(fallback).getAllByRole("listitem")[0]).toHaveAttribute(
      "title",
      expect.stringContaining("Provider B"),
    );
    expect(fallback.textContent).toContain(
      i18n.t("aiGatewayModelPoolStatusNoUsableKey"),
    );
    // The regular candidate count stays regular-only.
    expect(within(card).getByText(/1 candidate/i)).toBeInTheDocument();
  });
});
