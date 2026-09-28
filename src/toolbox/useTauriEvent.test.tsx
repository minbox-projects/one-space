import { act, renderHook, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useTauriEvent } from "@/toolbox/useTauriEvent";

const listenMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/event", () => ({ listen: listenMock }));

type Wrapper = (event: { payload: unknown }) => void;

function setTauriRuntime(present: boolean): void {
  if (present) {
    Object.defineProperty(window, "__TAURI_INTERNALS__", {
      value: {},
      configurable: true,
    });
  } else {
    delete (window as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
  }
}

function wrapperFromCall(index = 0): Wrapper {
  return listenMock.mock.calls[index]?.[1] as Wrapper;
}

describe("useTauriEvent", () => {
  beforeEach(() => {
    setTauriRuntime(true);
    listenMock.mockReset();
    listenMock.mockResolvedValue(vi.fn());
  });

  afterEach(() => {
    setTauriRuntime(true);
  });

  it("subscribes with the event name and invokes the handler with the payload", async () => {
    const handler = vi.fn();
    renderHook(() => useTauriEvent("file-sharing-updated", handler));

    await waitFor(() => expect(listenMock).toHaveBeenCalledTimes(1));
    expect(listenMock.mock.calls[0][0]).toBe("file-sharing-updated");

    const wrapper = wrapperFromCall();
    act(() => {
      wrapper({ payload: { running: true } });
    });
    expect(handler).toHaveBeenCalledWith({ running: true });
  });

  it("keeps a stable wrapper while invoking the current handler", async () => {
    const first = vi.fn();
    const second = vi.fn();
    const { rerender } = renderHook(
      ({ handler }: { handler: (payload: unknown) => void }) =>
        useTauriEvent("evt", handler),
      { initialProps: { handler: first } },
    );

    await waitFor(() => expect(listenMock).toHaveBeenCalledTimes(1));
    const wrapper = wrapperFromCall();
    const firstHandler = listenMock.mock.calls.length;

    rerender({ handler: second });
    expect(listenMock).toHaveBeenCalledTimes(firstHandler);
    expect(wrapperFromCall()).toBe(wrapper);

    act(() => {
      wrapper({ payload: "current" });
    });
    expect(second).toHaveBeenCalledWith("current");
    expect(first).not.toHaveBeenCalled();
  });

  it("unsubscribes on unmount", async () => {
    const unlisten = vi.fn();
    listenMock.mockResolvedValue(unlisten);
    const { unmount } = renderHook(() => useTauriEvent("evt", vi.fn()));

    await waitFor(() => expect(listenMock).toHaveBeenCalledTimes(1));
    unmount();
    await waitFor(() => expect(unlisten).toHaveBeenCalledTimes(1));
  });

  it("does not subscribe while disabled and unsubscribes when disabled later", async () => {
    const unlisten = vi.fn();
    listenMock.mockResolvedValue(unlisten);
    const { rerender } = renderHook(
      ({ enabled }: { enabled: boolean }) =>
        useTauriEvent("evt", vi.fn(), enabled),
      { initialProps: { enabled: false } },
    );

    expect(listenMock).not.toHaveBeenCalled();

    rerender({ enabled: true });
    await waitFor(() => expect(listenMock).toHaveBeenCalledTimes(1));

    rerender({ enabled: false });
    await waitFor(() => expect(unlisten).toHaveBeenCalledTimes(1));
  });

  it("does nothing outside Tauri and does not throw", async () => {
    setTauriRuntime(false);

    expect(() =>
      renderHook(() => useTauriEvent("evt", vi.fn())),
    ).not.toThrow();
    await act(async () => {
      await Promise.resolve();
    });

    expect(listenMock).not.toHaveBeenCalled();
  });
});
