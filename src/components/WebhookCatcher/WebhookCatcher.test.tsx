import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { WebhookCatcherTool } from "./index";
import { RequestStreamList } from "./RequestStreamList";
import { RequestInspector } from "./RequestInspector";
import { renderWithProviders } from "@/test/mocks/render";
import type { CapturedWebhookRequest } from "./types";

describe("WebhookCatcherTool", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    vi.useRealTimers();
  });

  it("renders the tool shell, header buttons, and empty waiting state", () => {
    renderWithProviders(<WebhookCatcherTool />);
    expect(
      screen.getByRole("heading", { name: /Webhook 捕获与重放/i }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /新建端点/i }),
    ).toBeInTheDocument();
    expect(screen.getByText(/等待接收外部请求/i)).toBeInTheDocument();
  });

  it("renders RequestStreamList with method badges", () => {
    const reqs: CapturedWebhookRequest[] = [
      {
        id: "req-1",
        endpointId: "ep-1",
        endpointName: "GitHub Hook",
        timestamp: 1700000000,
        method: "POST",
        path: "/hook/github",
        queryParams: {},
        headers: { "Content-Type": "application/json" },
        clientIp: "127.0.0.1",
        body: "{\"ref\":\"refs/heads/main\"}",
        bodyFormat: "json",
        isTruncated: false,
        detectedSignature: {
          provider: "GitHub",
          headerName: "X-Hub-Signature-256",
          headerValue: "sha256=xxx",
        },
      },
    ];

    renderWithProviders(
      <RequestStreamList
        requests={reqs}
        selectedId="req-1"
        onSelect={() => {}}
        onClear={() => {}}
      />,
    );

    expect(screen.getByText("POST")).toBeInTheDocument();
    expect(screen.getByText("/hook/github")).toBeInTheDocument();
    expect(screen.getByText(/GitHub 签名/i)).toBeInTheDocument();
  });

  it("renders RequestInspector and switches between Body and Headers tabs", async () => {
    const user = userEvent.setup();
    const req: CapturedWebhookRequest = {
      id: "req-1",
      endpointId: "ep-1",
      endpointName: "GitHub Hook",
      timestamp: 1700000000,
      method: "POST",
      path: "/hook/github",
      queryParams: { action: "sync" },
      headers: { "Content-Type": "application/json", Host: "127.0.0.1" },
      clientIp: "127.0.0.1",
      body: "{\"message\":\"hello\"}",
      bodyFormat: "json",
      isTruncated: false,
    };

    renderWithProviders(
      <RequestInspector
        request={req}
        onInstantReplay={async () => ({
          statusCode: 200,
          durationMs: 10,
          headers: {},
          responseBody: "ok",
          executedAt: 1700000010,
        })}
        onOpenReplayDrawer={() => {}}
      />,
    );

    // 默认展示 Body
    expect(screen.getByText(/"message": "hello"/i)).toBeInTheDocument();

    // 切换到 Headers 标签
    const headersTab = screen.getByRole("button", { name: /Headers/i });
    await user.click(headersTab);

    expect(screen.getByText("Content-Type")).toBeInTheDocument();
    expect(screen.getByText("application/json")).toBeInTheDocument();
  });
});
