import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import i18n from "@/i18n";
import { ShortLinkTool } from "@/components/ShortLinkTool";
import { SHORT_LINK_HISTORY_KEY, type ShortLinkHistoryRecord } from "@/lib/shortLinkHistory";
import { renderWithProviders } from "@/test/mocks/render";
import {
  invokeMock,
  resetTauriMocks,
  shortLinkConfigStatusMock,
  shortLinkCreateMock,
  shortLinkDeleteTokenMock,
  shortLinkSaveTokenMock,
} from "@/test/mocks/tauri";

function deferred<T>() {
  let resolve: (value: T) => void;
  let reject: (reason?: unknown) => void;
  const promise = new Promise<T>((nextResolve, nextReject) => {
    resolve = nextResolve;
    reject = nextReject;
  });
  return { promise, resolve: resolve!, reject: reject! };
}

function historyRecord(index: number, createdAt: string): ShortLinkHistoryRecord {
  return {
    id: `history-${index}`,
    longUrl: `https://example.com/long/${index}`,
    shortUrl: `https://tinyurl.com/history-${index}`,
    createdAt,
  };
}

async function renderConfigured() {
  shortLinkConfigStatusMock.mockResolvedValue({
    configured: true,
    token: "status-response-must-not-expose-token",
  });
  renderWithProviders(<ShortLinkTool />);
  await screen.findByText("API Token configured");
}

async function generate(url = "https://example.com/a/long/path") {
  const user = userEvent.setup();
  const input = screen.getByLabelText("Long URL");
  await user.clear(input);
  await user.type(input, url);
  await user.click(screen.getByRole("button", { name: "Generate short link" }));
  return { user, input };
}

describe("ShortLinkTool", () => {
  beforeEach(async () => {
    vi.restoreAllMocks();
    localStorage.clear();
    resetTauriMocks();
    await i18n.changeLanguage("en");
    let uuidIndex = 0;
    vi.spyOn(crypto, "randomUUID").mockImplementation(
      () => `00000000-0000-4000-8000-${String(++uuidIndex).padStart(12, "0")}`,
    );
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText: vi.fn().mockResolvedValue(undefined) },
    });
  });

  it("loads credential status and history concurrently without exposing a returned token", async () => {
    const status = deferred<{ configured: boolean; token: string }>();
    shortLinkConfigStatusMock.mockReturnValue(status.promise);
    localStorage.setItem(
      SHORT_LINK_HISTORY_KEY,
      JSON.stringify([historyRecord(1, "2026-01-01T00:00:00.000Z")]),
    );

    renderWithProviders(<ShortLinkTool />);

    expect(await screen.findByText("https://tinyurl.com/history-1")).toBeInTheDocument();
    expect(screen.getByText("Checking credential status...")).toBeInTheDocument();
    expect(shortLinkConfigStatusMock).toHaveBeenCalledTimes(1);
    status.resolve({ configured: true, token: "never-render-this-token" });

    expect(await screen.findByText("API Token configured")).toBeInTheDocument();
    expect(screen.queryByText(/never-render-this-token/)).not.toBeInTheDocument();
  });

  it("saves, replaces, masks, clears, and deletes a token without removing history", async () => {
    const user = userEvent.setup();
    localStorage.setItem(
      SHORT_LINK_HISTORY_KEY,
      JSON.stringify([historyRecord(1, "2026-01-01T00:00:00.000Z")]),
    );
    renderWithProviders(<ShortLinkTool />);

    const tokenInput = await screen.findByLabelText("TinyURL API Token");
    expect(tokenInput).toHaveAttribute("type", "password");
    await user.type(tokenInput, "first-test-token");
    await user.click(screen.getByRole("button", { name: "Show Token" }));
    expect(tokenInput).toHaveAttribute("type", "text");
    await user.click(screen.getByRole("button", { name: "Save Token" }));

    await waitFor(() => expect(shortLinkSaveTokenMock).toHaveBeenCalledWith("first-test-token"));
    expect(screen.queryByDisplayValue("first-test-token")).not.toBeInTheDocument();
    expect(screen.queryByText("first-test-token")).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Replace Token" }));
    const replacementInput = screen.getByLabelText("TinyURL API Token");
    expect(replacementInput).toHaveValue("");
    expect(replacementInput).toHaveAttribute("type", "password");
    await user.type(replacementInput, "replacement-test-token");
    await user.click(screen.getByRole("button", { name: "Save Token" }));
    await waitFor(() =>
      expect(shortLinkSaveTokenMock).toHaveBeenLastCalledWith("replacement-test-token"),
    );
    expect(screen.queryByDisplayValue("replacement-test-token")).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Delete saved Token" }));
    expect(screen.getByText(/Existing short links and local history will remain available/)).toBeInTheDocument();
    await user.click(screen.getAllByRole("button", { name: "Delete saved Token" }).at(-1)!);

    await waitFor(() => expect(shortLinkDeleteTokenMock).toHaveBeenCalledTimes(1));
    expect(await screen.findByText("API Token not configured")).toBeInTheDocument();
    expect(screen.getByText("https://tinyurl.com/history-1")).toBeInTheDocument();
  });

  it("blocks a blank token before IPC", async () => {
    const user = userEvent.setup();
    renderWithProviders(<ShortLinkTool />);

    await user.type(await screen.findByLabelText("TinyURL API Token"), "   ");
    await user.click(screen.getByRole("button", { name: "Save Token" }));

    expect(screen.getByRole("alert")).toHaveTextContent("Enter an API Token before saving.");
    expect(shortLinkSaveTokenMock).not.toHaveBeenCalled();
  });

  it("localizes the Token visibility control in both masked and visible states", async () => {
    await i18n.changeLanguage("zh");
    const user = userEvent.setup();
    renderWithProviders(<ShortLinkTool />);

    const tokenInput = await screen.findByLabelText("TinyURL API Token");
    const showToken = screen.getByRole("button", { name: "显示 Token" });
    expect(tokenInput).toHaveAttribute("type", "password");
    expect(showToken).toHaveAttribute("title", "显示 Token");

    await user.click(showToken);

    expect(tokenInput).toHaveAttribute("type", "text");
    expect(screen.getByRole("button", { name: "隐藏 Token" })).toHaveAttribute(
      "title",
      "隐藏 Token",
    );
  });

  it.each(["", "relative/path", "ftp://example.com/file", "https:///"])(
    "rejects invalid URL %j without create IPC",
    async (invalidUrl) => {
      await renderConfigured();
      const user = userEvent.setup();
      const input = screen.getByLabelText("Long URL");
      if (invalidUrl) fireEvent.change(input, { target: { value: invalidUrl } });

      await user.click(screen.getByRole("button", { name: "Generate short link" }));

      expect(await screen.findByRole("alert")).toHaveTextContent(
        "Enter a valid HTTP or HTTPS URL.",
      );
      expect(shortLinkCreateMock).not.toHaveBeenCalled();
      expect(invokeMock).not.toHaveBeenCalledWith("short_link_create", expect.anything());
    },
  );

  it("prevents duplicate creates, preserves both URLs, and keeps the result when input changes", async () => {
    const pending = deferred<{ longUrl: string; shortUrl: string }>();
    shortLinkCreateMock.mockReturnValue(pending.promise);
    await renderConfigured();

    const { user, input } = await generate();
    const submit = screen.getByRole("button", { name: "Generating..." });
    fireEvent.click(submit);
    expect(shortLinkCreateMock).toHaveBeenCalledTimes(1);
    expect(submit).toBeDisabled();

    pending.resolve({
      longUrl: "https://example.com/a/long/path",
      shortUrl: "https://tinyurl.com/current-result",
    });

    const result = await screen.findByTestId("short-link-current-result");
    expect(result).toHaveTextContent("https://example.com/a/long/path");
    expect(result).toHaveTextContent("https://tinyurl.com/current-result");
    expect(screen.getByRole("button", { name: "Generate short link" })).toBeEnabled();
    await user.clear(input);
    await user.type(input, "https://example.com/next");
    expect(result).toHaveTextContent("https://tinyurl.com/current-result");
  });

  it("accepts an HTTP URL with a host and sends it through create IPC", async () => {
    shortLinkCreateMock.mockResolvedValue({
      longUrl: "http://localhost:3000/path",
      shortUrl: "https://tinyurl.com/http-result",
    });
    await renderConfigured();

    await generate("http://localhost:3000/path");

    expect(await screen.findAllByText("https://tinyurl.com/http-result")).toHaveLength(2);
    expect(shortLinkCreateMock).toHaveBeenCalledWith("http://localhost:3000/path");
  });

  it("keeps a successful result copyable when history persistence fails", async () => {
    shortLinkCreateMock.mockResolvedValue({
      longUrl: "https://example.com/persist-failure",
      shortUrl: "https://tinyurl.com/still-visible",
    });
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new DOMException("quota", "QuotaExceededError");
    });
    await renderConfigured();

    await generate("https://example.com/persist-failure");

    const result = await screen.findByTestId("short-link-current-result");
    expect(result).toHaveTextContent("https://tinyurl.com/still-visible");
    expect(screen.getByText(/history update could not be saved/)).toBeInTheDocument();
    const writeText = vi
      .spyOn(navigator.clipboard, "writeText")
      .mockResolvedValue(undefined);
    await userEvent.setup().click(
      within(result).getByRole("button", { name: "Copy short link" }),
    );
    expect(writeText).toHaveBeenCalledWith("https://tinyurl.com/still-visible");
    expect(await screen.findByText("Short link copied.")).toBeInTheDocument();
  });

  it("reports clipboard failure for current results without removing them", async () => {
    shortLinkCreateMock.mockResolvedValue({
      longUrl: "https://example.com/copy-failure",
      shortUrl: "https://tinyurl.com/copy-failure",
    });
    await renderConfigured();
    await generate("https://example.com/copy-failure");
    const result = await screen.findByTestId("short-link-current-result");
    vi.spyOn(navigator.clipboard, "writeText").mockRejectedValueOnce(new Error("denied"));

    await userEvent.setup().click(
      within(result).getByRole("button", { name: "Copy short link" }),
    );

    expect(await screen.findByText("Unable to copy the short link to the clipboard.")).toBeInTheDocument();
    expect(result).toHaveTextContent("https://tinyurl.com/copy-failure");
  });

  it("reloads at most 50 newest records and copies, deletes, and clears only local history", async () => {
    const records = Array.from({ length: 52 }, (_, index) =>
      historyRecord(index, new Date(Date.UTC(2026, 0, index + 1)).toISOString()),
    );
    localStorage.setItem(SHORT_LINK_HISTORY_KEY, JSON.stringify(records));
    await renderConfigured();

    const items = await screen.findAllByTestId("short-link-history-item");
    expect(items).toHaveLength(50);
    expect(items[0]).toHaveTextContent("https://tinyurl.com/history-51");
    expect(items.at(-1)).toHaveTextContent("https://tinyurl.com/history-2");

    const user = userEvent.setup();
    const writeText = vi
      .spyOn(navigator.clipboard, "writeText")
      .mockResolvedValue(undefined);
    await user.click(within(items[0]).getByRole("button", { name: "Copy history item" }));
    expect(writeText).toHaveBeenCalledWith("https://tinyurl.com/history-51");
    expect(await screen.findByText("Short link copied.")).toBeInTheDocument();

    writeText.mockRejectedValueOnce(new Error("denied"));
    await user.click(within(items[1]).getByRole("button", { name: "Copy history item" }));
    expect(await screen.findByText(/Unable to copy the short link/)).toBeInTheDocument();
    expect(items[1]).toHaveTextContent("https://tinyurl.com/history-50");

    await user.click(within(items[0]).getByRole("button", { name: "Delete local record" }));
    expect(screen.getByText(/TinyURL link will remain active remotely/)).toBeInTheDocument();
    await user.click(screen.getAllByRole("button", { name: "Delete local record" }).at(-1)!);
    await waitFor(() => expect(screen.getAllByTestId("short-link-history-item")).toHaveLength(49));

    await user.click(screen.getByRole("button", { name: "Clear local history" }));
    expect(screen.getByText(/TinyURL links will remain active remotely/)).toBeInTheDocument();
    await user.click(screen.getAllByRole("button", { name: "Clear local history" }).at(-1)!);

    expect(await screen.findByText(/Short links created on this device will appear here/)).toBeInTheDocument();
    expect(localStorage.getItem(SHORT_LINK_HISTORY_KEY)).toBeNull();
    expect(shortLinkCreateMock).not.toHaveBeenCalled();
    expect(shortLinkDeleteTokenMock).not.toHaveBeenCalled();
    expect(
      invokeMock.mock.calls.filter(([command]) => command !== "short_link_config_status"),
    ).toEqual([]);
  });

  it("keeps history visible and avoids success feedback when delete or clear persistence fails", async () => {
    localStorage.setItem(
      SHORT_LINK_HISTORY_KEY,
      JSON.stringify([historyRecord(1, "2026-01-01T00:00:00.000Z")]),
    );
    await renderConfigured();
    const user = userEvent.setup();
    const record = await screen.findByTestId("short-link-history-item");
    const setItem = vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new DOMException("quota", "QuotaExceededError");
    });

    await user.click(within(record).getByRole("button", { name: "Delete local record" }));
    await user.click(screen.getAllByRole("button", { name: "Delete local record" }).at(-1)!);

    expect(await screen.findByText(/history update could not be saved/)).toBeInTheDocument();
    expect(screen.getByTestId("short-link-history-item")).toHaveTextContent(
      "https://tinyurl.com/history-1",
    );
    expect(screen.queryByText(/Local record deleted/)).not.toBeInTheDocument();

    setItem.mockRestore();
    vi.spyOn(Storage.prototype, "removeItem").mockImplementation(() => {
      throw new DOMException("denied", "SecurityError");
    });
    await user.click(screen.getByRole("button", { name: "Clear local history" }));
    await user.click(screen.getAllByRole("button", { name: "Clear local history" }).at(-1)!);

    expect(screen.getByTestId("short-link-history-item")).toHaveTextContent(
      "https://tinyurl.com/history-1",
    );
    expect(screen.queryByText(/Local history cleared/)).not.toBeInTheDocument();
    expect(shortLinkCreateMock).not.toHaveBeenCalled();
    expect(shortLinkDeleteTokenMock).not.toHaveBeenCalled();
  });

  it("reports damaged history once while credential configuration remains usable", async () => {
    localStorage.setItem(SHORT_LINK_HISTORY_KEY, "{");
    shortLinkConfigStatusMock.mockResolvedValue({ configured: true });

    renderWithProviders(<ShortLinkTool />);

    expect(await screen.findByText(/Damaged local history was discarded/)).toBeInTheDocument();
    expect(screen.getAllByText(/Damaged local history was discarded/)).toHaveLength(1);
    expect(await screen.findByText("API Token configured")).toBeInTheDocument();
  });

  it("distinguishes damaged history cleanup failure from history write failure", async () => {
    localStorage.setItem(SHORT_LINK_HISTORY_KEY, "{");
    vi.spyOn(Storage.prototype, "removeItem").mockImplementation(() => {
      throw new DOMException("denied", "SecurityError");
    });
    shortLinkConfigStatusMock.mockResolvedValue({ configured: true });

    renderWithProviders(<ShortLinkTool />);

    const feedback = await screen.findByText(
      "Unable to remove damaged local history from this device. No remote TinyURL data was changed.",
    );
    expect(feedback).not.toHaveTextContent(/Token/i);
    expect(screen.queryByText(/history update could not be saved/i)).not.toBeInTheDocument();
    expect(shortLinkCreateMock).not.toHaveBeenCalled();
    expect(
      invokeMock.mock.calls.filter(([command]) => command !== "short_link_config_status"),
    ).toEqual([]);
  });

  it("isolates history read failure from credential status and generation", async () => {
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new DOMException("denied", "SecurityError");
    });
    shortLinkCreateMock.mockResolvedValue({
      longUrl: "https://example.com/read-failed",
      shortUrl: "https://tinyurl.com/read-failed",
    });
    await renderConfigured();

    expect(await screen.findByText(/Unable to read local history/)).toBeInTheDocument();
    await generate("https://example.com/read-failed");
    expect(await screen.findByText("https://tinyurl.com/read-failed")).toBeInTheDocument();
  });

  it("isolates credential status failure from loaded history", async () => {
    localStorage.setItem(
      SHORT_LINK_HISTORY_KEY,
      JSON.stringify([historyRecord(1, "2026-01-01T00:00:00.000Z")]),
    );
    shortLinkConfigStatusMock.mockRejectedValue({
      code: "storage_error",
      message: "sensitive-status-diagnostic",
    });

    renderWithProviders(<ShortLinkTool />);

    expect(await screen.findByText("https://tinyurl.com/history-1")).toBeInTheDocument();
    expect(await screen.findByText(/Unable to access the encrypted TinyURL credential/)).toBeInTheDocument();
    expect(screen.getByLabelText("TinyURL API Token")).toBeInTheDocument();
    expect(screen.queryByText(/sensitive-status-diagnostic/)).not.toBeInTheDocument();
  });

  it.each([
    ["not_configured", "Configure a TinyURL API Token before generating a short link."],
    ["invalid_url", "Enter a valid HTTP or HTTPS URL."],
    ["authentication_failed", "TinyURL rejected the saved API Token."],
    ["rate_limited", "TinyURL rate limit reached."],
    ["request_rejected", "TinyURL rejected this request."],
    ["service_unavailable", "TinyURL is currently unavailable."],
    ["network_error", "Could not reach TinyURL."],
    ["invalid_response", "TinyURL returned an invalid response."],
    ["storage_error", "Unable to access the encrypted TinyURL credential"],
  ] as const)("maps backend error %s by code without exposing message", async (code, expected) => {
    shortLinkCreateMock.mockRejectedValue({
      code,
      message: "secret-token-from-backend-must-not-render",
    });
    await renderConfigured();

    const { input } = await generate(`https://example.com/error/${code}`);

    expect(await screen.findByText(new RegExp(expected.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")))).toBeInTheDocument();
    expect(input).toHaveValue(`https://example.com/error/${code}`);
    expect(screen.queryByText(/secret-token-from-backend-must-not-render/)).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Generate short link" })).toBeEnabled();
    if (code === "not_configured") {
      expect(screen.getByLabelText("TinyURL API Token")).toBeInTheDocument();
    }
  });
});
