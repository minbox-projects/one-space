import { act, fireEvent, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { AiRequestCaptureTool } from "@/components/AiRequestCaptureTool";
import { renderWithProviders } from "@/test/mocks/render";
import { dialogSaveMock, invokeMock, listenMock, resetTauriMocks } from "@/test/mocks/tauri";

const config = { enabled: true, port: 17688, upstreamBaseUrl: "https://api.example.com/v1" };
const status = { running: true, listenAddress: "127.0.0.1", port: 17688, lastError: null };
const item = {
  id: "capture-1",
  startedAt: 1710000000000,
  completedAt: 1710000000200,
  state: "completed",
  method: "POST",
  requestPathAndQuery: "/chat/completions?stream=false",
  upstreamUrl: "https://api.example.com/v1/chat/completions?stream=false",
  responseStatus: 200,
  durationMs: 200,
  provider: "openai",
  model: "gpt-4o",
  inputTokens: 12,
  outputTokens: 8,
  totalTokens: 20,
};
const detail = {
  ...item,
  httpVersion: "HTTP/1.1",
  requestHeaders: [{ name: "Authorization", values: ["Bearer secret"] }],
  requestBody: { data: '{"model":"gpt-4o"}', encoding: null, capturedBytes: 18, totalBytes: 18, truncated: false },
  responseHeaders: [{ name: "Content-Type", values: ["application/json"] }],
  responseBody: { data: '{"id":"chatcmpl-1","ok":true}', encoding: null, capturedBytes: 29, totalBytes: 29, truncated: false },
  error: null,
};

function installCaptureResponses() {
  invokeMock.mockImplementation(async (command: string, args?: { id?: string; query?: Record<string, unknown> }) => {
    if (command === "ai_request_capture_get_config") return config;
    if (command === "ai_request_capture_status") return status;
    if (command === "ai_request_capture_list") {
      return { items: [item], total: 31, page: args?.query?.page ?? 1, pageSize: args?.query?.pageSize ?? 20 };
    }
    if (command === "ai_request_capture_get") return { ...detail, id: args?.id ?? item.id };
    if (command === "ai_request_capture_generate_curl") return { command: "curl -X POST 'https://api.example.com/v1/chat/completions'", complete: true, warning: null };
    if (command === "ai_request_capture_save_config") return { config, status, validationErrors: [] };
    throw new Error(`Unhandled command: ${command}`);
  });
}

describe("AiRequestCaptureTool", () => {
  beforeEach(() => {
    resetTauriMocks();
    installCaptureResponses();
  });

  it("renders the configuration, persistent plaintext warning, runtime endpoints, and server-side request filters", async () => {
    renderWithProviders(<AiRequestCaptureTool />);

    expect(await screen.findByRole("alert")).toHaveTextContent(/plaintext|明文/i);
    expect(screen.getByDisplayValue("17688")).toBeInTheDocument();
    expect(screen.getByDisplayValue("https://api.example.com/v1")).toBeInTheDocument();
    expect(screen.getByText("http://127.0.0.1:17688")).toBeInTheDocument();

    fireEvent.change(screen.getByLabelText(/Search requests|搜索请求/), { target: { value: "chat" } });
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("ai_request_capture_list", {
        query: expect.objectContaining({ search: "chat", page: 1, pageSize: 20 }),
      });
    });
  });

  it("loads selected request details, formats JSON bodies, and shows generated cURL", async () => {
    const user = userEvent.setup();
    renderWithProviders(<AiRequestCaptureTool />);

    await user.click(await screen.findByRole("button", { name: /POST.*chat\/completions/i }));
    await user.click(screen.getByRole("tab", { name: /Request|请求/ }));
    expect(await screen.findByText("Authorization")).toBeInTheDocument();
    expect(screen.getByText("Bearer secret")).toBeInTheDocument();
    await user.click(screen.getByRole("tab", { name: /Body|正文/ }));
    expect(screen.getByText(/"model": "gpt-4o"/)).toBeInTheDocument();

    await user.click(screen.getByRole("tab", { name: /Response|响应/ }));
    await user.click(screen.getByRole("tab", { name: /Body|正文/ }));
    expect(screen.getByText(/"id": "chatcmpl-1"/)).toBeInTheDocument();
    await user.click(screen.getByRole("tab", { name: /Overview|概览/ }));
    expect(await screen.findByText(/curl -X POST/)).toBeInTheDocument();
  });

  it("debounces capture update events and refreshes the selected detail without polling", async () => {
    vi.useFakeTimers();
    try {
      let updatedHandler: ((event: { payload: { kind: string } }) => void) | undefined;
      listenMock.mockImplementation(async (...args: unknown[]) => {
        const [event, handler] = args as [string, (event: { payload: { kind: string } }) => void];
        if (event === "ai-request-capture-updated") updatedHandler = handler as (event: { payload: { kind: string } }) => void;
        return vi.fn();
      });
      renderWithProviders(<AiRequestCaptureTool />);
      await act(async () => {
        await vi.advanceTimersByTimeAsync(0);
      });
      invokeMock.mockClear();

      act(() => {
        updatedHandler?.({ payload: { kind: "completed" } });
        updatedHandler?.({ payload: { kind: "completed" } });
      });
      await act(async () => {
        await vi.advanceTimersByTimeAsync(300);
      });

      expect(invokeMock.mock.calls.filter(([command]) => command === "ai_request_capture_list")).toHaveLength(1);
    } finally {
      vi.useRealTimers();
    }
  });

  it("does not let a stale detail response replace a newer selection", async () => {
    let resolveFirst!: (value: typeof detail) => void;
    const firstDetail = new Promise<typeof detail>((resolve) => {
      resolveFirst = resolve;
    });
    invokeMock.mockImplementation(async (command: string, args?: { id?: string; query?: Record<string, unknown> }) => {
      if (command === "ai_request_capture_get_config") return config;
      if (command === "ai_request_capture_status") return status;
      if (command === "ai_request_capture_list") return { items: [item, { ...item, id: "capture-2", model: "gpt-4.1" }], total: 2, page: args?.query?.page ?? 1, pageSize: 20 };
      if (command === "ai_request_capture_get" && args?.id === "capture-1") return firstDetail;
      if (command === "ai_request_capture_get") return { ...detail, id: "capture-2", model: "gpt-4.1", requestHeaders: [{ name: "X-Request", values: ["newer"] }] };
      if (command === "ai_request_capture_generate_curl") return { command: "curl", complete: true, warning: null };
      throw new Error(`Unhandled command: ${command}`);
    });
    const user = userEvent.setup();
    renderWithProviders(<AiRequestCaptureTool />);

    await user.click(await screen.findByRole("button", { name: /POST.*chat\/completions.*gpt-4o/i }));
    await user.click(screen.getByRole("button", { name: /POST.*chat\/completions.*gpt-4\.1/i }));
    await user.click(screen.getByRole("tab", { name: /Request|请求/ }));
    expect(await screen.findByText("newer")).toBeInTheDocument();
    await act(async () => {
      resolveFirst(detail);
    });
    expect(screen.getByText("newer")).toBeInTheDocument();
  });

  it("refreshes the workspace and reports success", async () => {
    const user = userEvent.setup();
    renderWithProviders(<AiRequestCaptureTool />);
    await screen.findByRole("button", { name: /POST.*chat\/completions/i });
    invokeMock.mockClear();

    await user.click(screen.getByRole("button", { name: /Refresh|刷新/ }));

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("ai_request_capture_get_config");
      expect(invokeMock).toHaveBeenCalledWith("ai_request_capture_status");
      expect(invokeMock).toHaveBeenCalledWith("ai_request_capture_list", expect.anything());
    });
    expect(await screen.findByRole("status")).toHaveTextContent(/Requests refreshed|请求已刷新/);
  });

  it("does not export when choosing a save path or the sensitive-data confirmation is cancelled", async () => {
    const user = userEvent.setup();
    dialogSaveMock.mockResolvedValueOnce(null);
    renderWithProviders(<AiRequestCaptureTool />);

    await user.click(await screen.findByRole("button", { name: /Export HAR|导出 HAR/ }));
    expect(invokeMock).not.toHaveBeenCalledWith("ai_request_capture_export_har", expect.anything());

    dialogSaveMock.mockResolvedValueOnce("/tmp/captures.har");
    await user.click(screen.getByRole("button", { name: /Export HAR|导出 HAR/ }));
    expect(await screen.findByText(/plaintext authentication|明文鉴权/i)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /Cancel|取消/ }));
    expect(invokeMock).not.toHaveBeenCalledWith("ai_request_capture_export_har", expect.anything());
  });

  it("exports every current filter match after confirming sensitive HAR contents and refreshes the list", async () => {
    const user = userEvent.setup();
    dialogSaveMock.mockResolvedValue("/tmp/captures.har");
    invokeMock.mockImplementation(async (command: string, args?: { query?: Record<string, unknown> }) => {
      if (command === "ai_request_capture_get_config") return config;
      if (command === "ai_request_capture_status") return status;
      if (command === "ai_request_capture_list") return { items: [item], total: 31, page: args?.query?.page ?? 1, pageSize: args?.query?.pageSize ?? 20 };
      if (command === "ai_request_capture_export_har") return { outputPath: "/tmp/captures.har", exported: 31 };
      throw new Error(`Unhandled command: ${command}`);
    });
    renderWithProviders(<AiRequestCaptureTool />);

    fireEvent.change(await screen.findByLabelText(/Search requests|搜索请求/), { target: { value: "chat" } });
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("ai_request_capture_list", {
      query: expect.objectContaining({ search: "chat" }),
    }));
    invokeMock.mockClear();

    await user.click(screen.getByRole("button", { name: /Export HAR|导出 HAR/ }));
    await user.click(await screen.findByRole("button", { name: /^Export$|^导出$/ }));

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("ai_request_capture_export_har", {
        input: {
          outputPath: "/tmp/captures.har",
          query: expect.objectContaining({ search: "chat", page: 1, pageSize: 20 }),
        },
      });
    });
    expect(await screen.findByText(/Exported 31 requests|已导出 31 条请求/)).toBeInTheDocument();
    expect(invokeMock).toHaveBeenCalledWith("ai_request_capture_list", expect.anything());
  });

  it("shows an export error without clearing the persistent plaintext warning", async () => {
    const user = userEvent.setup();
    dialogSaveMock.mockResolvedValue("/tmp/captures.har");
    invokeMock.mockImplementation(async (command: string, args?: { query?: Record<string, unknown> }) => {
      if (command === "ai_request_capture_get_config") return config;
      if (command === "ai_request_capture_status") return status;
      if (command === "ai_request_capture_list") return { items: [item], total: 1, page: args?.query?.page ?? 1, pageSize: args?.query?.pageSize ?? 20 };
      if (command === "ai_request_capture_export_har") throw new Error("disk full");
      throw new Error(`Unhandled command: ${command}`);
    });
    renderWithProviders(<AiRequestCaptureTool />);

    await user.click(await screen.findByRole("button", { name: /Export HAR|导出 HAR/ }));
    await user.click(await screen.findByRole("button", { name: /^Export$|^导出$/ }));

    expect(await screen.findByText(/disk full/)).toBeInTheDocument();
    expect(screen.getByText(/Captured headers and bodies are stored and displayed as plaintext|抓取的请求头和正文会以明文/i)).toBeInTheDocument();
  });

  it("clears all history only after confirmation, then resets the selection and reloads page one", async () => {
    const user = userEvent.setup();
    let cleared = false;
    invokeMock.mockImplementation(async (command: string, args?: { id?: string; query?: Record<string, unknown> }) => {
      if (command === "ai_request_capture_get_config") return config;
      if (command === "ai_request_capture_status") return status;
      if (command === "ai_request_capture_list") return { items: cleared ? [] : [item], total: cleared ? 0 : 1, page: args?.query?.page ?? 1, pageSize: args?.query?.pageSize ?? 20 };
      if (command === "ai_request_capture_get") return { ...detail, id: args?.id ?? item.id };
      if (command === "ai_request_capture_generate_curl") return { command: "curl", complete: true, warning: null };
      if (command === "ai_request_capture_clear") {
        cleared = true;
        return { cleared: 1 };
      }
      throw new Error(`Unhandled command: ${command}`);
    });
    renderWithProviders(<AiRequestCaptureTool />);

    await user.click(await screen.findByRole("button", { name: /POST.*chat\/completions/i }));
    expect(await screen.findByRole("button", { name: /Copy cURL|复制 cURL/ })).toBeInTheDocument();
    await user.click(await screen.findByRole("button", { name: /Clear history|清空历史/ }));
    expect(await screen.findByText(/including in-progress requests|包括进行中的请求/i)).toBeInTheDocument();
    expect(invokeMock).not.toHaveBeenCalledWith("ai_request_capture_clear");
    await user.click(screen.getByRole("button", { name: /^Clear$|^清空$/ }));

    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("ai_request_capture_clear"));
    expect(await screen.findByText(/Select a request|选择一个请求/)).toBeInTheDocument();
    expect(invokeMock).toHaveBeenCalledWith("ai_request_capture_list", {
      query: expect.objectContaining({ page: 1, pageSize: 20 }),
    });
  });

  it("copies a complete cURL and shows copy feedback", async () => {
    const user = userEvent.setup();
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText } });
    renderWithProviders(<AiRequestCaptureTool />);

    await user.click(await screen.findByRole("button", { name: /POST.*chat\/completions/i }));
    await user.click(await screen.findByRole("button", { name: /Copy cURL|复制 cURL/ }));

    expect(writeText).toHaveBeenCalledWith("curl -X POST 'https://api.example.com/v1/chat/completions'");
    expect(screen.getByRole("button", { name: /Copy cURL|复制 cURL/ }).querySelector("svg")).toHaveClass("lucide-check");
  });

  it("requires acknowledgement before copying an incomplete cURL with its warning comment", async () => {
    const user = userEvent.setup();
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText } });
    invokeMock.mockImplementation(async (command: string, args?: { id?: string; query?: Record<string, unknown> }) => {
      if (command === "ai_request_capture_get_config") return config;
      if (command === "ai_request_capture_status") return status;
      if (command === "ai_request_capture_list") return { items: [item], total: 1, page: args?.query?.page ?? 1, pageSize: args?.query?.pageSize ?? 20 };
      if (command === "ai_request_capture_get") return { ...detail, id: args?.id ?? item.id, state: "interrupted" };
      if (command === "ai_request_capture_generate_curl") return { command: "# WARNING: body truncated\ncurl -H 'Authorization: Bearer secret'", complete: false, warning: "body truncated" };
      throw new Error(`Unhandled command: ${command}`);
    });
    renderWithProviders(<AiRequestCaptureTool />);

    await user.click(await screen.findByRole("button", { name: /POST.*chat\/completions/i }));
    await user.click(await screen.findByRole("button", { name: /Copy cURL|复制 cURL/ }));
    expect(await screen.findByText(/This cURL contains real authentication headers.*body truncated|此 cURL 包含真实的鉴权请求头.*body truncated/i)).toBeInTheDocument();
    expect(writeText).not.toHaveBeenCalled();
    await user.click(screen.getByRole("button", { name: /Copy anyway|仍然复制/ }));

    expect(writeText).toHaveBeenCalledWith("# WARNING: body truncated\ncurl -H 'Authorization: Bearer secret'");
  });
});
