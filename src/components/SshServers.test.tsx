import { act, fireEvent, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import { SshServers } from "@/components/SshServers";
import { renderWithProviders } from "@/test/mocks/render";
import {
  dialogOpenMock,
  invokeMock,
  resetTauriMocks,
} from "@/test/mocks/tauri";

async function settle() {
  await act(async () => {
    await Promise.resolve();
    await Promise.resolve();
    await Promise.resolve();
  });
}

const hosts = [
  { name: "alpha", host_name: "alpha.example", user: "root", port: 22 },
  { name: "beta", host_name: "beta.example", user: "root", port: 22 },
];

const history = [
  {
    id: "h-alpha",
    type: "config",
    name: "alpha",
    host_name: "alpha.example",
    user: "root",
    port: 22,
    last_connected: 0,
  },
  {
    id: "h-beta",
    type: "config",
    name: "beta",
    host_name: "beta.example",
    user: "root",
    port: 22,
    last_connected: 0,
  },
];

describe("SshServers", () => {
  beforeEach(() => {
    resetTauriMocks();
  });

  it("历史过滤仍然隐藏被忽略的条目", async () => {
    invokeMock.mockImplementation(
      async (command: string, args?: Record<string, unknown>) => {
        if (command === "get_ssh_hosts") return hosts;
        if (command === "get_secret") {
          if (args?.key === "onespace_ssh_history") {
            return JSON.stringify(history);
          }
          if (args?.key === "onespace_ssh_ignored") {
            return JSON.stringify(["beta"]);
          }
          if (args?.key === "onespace_ssh_favorites") {
            return JSON.stringify([]);
          }
        }
        return null;
      },
    );

    renderWithProviders(<SshServers />);
    await settle();

    fireEvent.click(screen.getByRole("button", { name: /历史|History/ }));

    expect(await screen.findByText("alpha")).toBeInTheDocument();
    expect(screen.queryByText("beta")).not.toBeInTheDocument();
  });

  it("密钥文件选择失败时展示可见错误而非仅写控制台", async () => {
    dialogOpenMock.mockRejectedValue(new Error("key file unavailable"));
    invokeMock.mockImplementation(
      async (command: string) => {
        if (command === "get_ssh_hosts") return [];
        return null;
      },
    );

    const { container } = renderWithProviders(<SshServers />);
    await settle();

    fireEvent.click(screen.getByRole("button", { name: /自定义|Custom/ }));
    fireEvent.click(
      screen.getByRole("radio", { name: /身份验证密钥文件|Identity Key File/ }),
    );
    fireEvent.click(screen.getByRole("button", { name: /浏览|Browse/ }));
    await settle();

    const alert = screen.queryByRole("alert");
    const legacyBanner = container.querySelector(".bg-destructive\\/15");
    const errorNode = alert ?? legacyBanner;

    expect(errorNode).not.toBeNull();
    expect(errorNode?.textContent?.trim().length ?? 0).toBeGreaterThan(0);
  });
});
