import { fireEvent, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import { Launcher } from "@/components/Launcher";
import { renderWithProviders } from "@/test/mocks/render";
import { resetTauriMocks, invokeMock } from "@/test/mocks/tauri";
import {
  resetMessageMocks,
  safeRecordMessageMock,
} from "@/test/mocks/messages";

const launcherItems = [
  {
    id: "script-1",
    name: "Danger Script",
    type: "script",
    target: "rm -rf ./tmp",
    pinned: false,
    pin_order: 0,
    launch_count: 0,
    trusted: false,
    created_at: 1,
    updated_at: 1,
  },
];

describe("Launcher", () => {
  beforeEach(() => {
    resetTauriMocks();
    resetMessageMocks();
    invokeMock.mockImplementation(async (command: string, args?: unknown) => {
      if (command === "launcher_list") {
        return { data: launcherItems };
      }
      if (command === "ssh_tunnels_snapshot") {
        return { groups: [], tunnels: [], runtime: [] };
      }
      if (command === "launcher_mark_launched") {
        return null;
      }
      if (command === "launcher_execute") {
        return null;
      }
      if (command === "launcher_set_trust") {
        return null;
      }
      if (command === "dashboard_counts") {
        return { data: {} };
      }
      if (command === "get_storage_config") {
        return {};
      }
      if (command === "protocol_router_status") {
        return { running: false, enabled: false, port: 0, route_count: 0 };
      }
      throw new Error(`Unhandled command: ${command} ${JSON.stringify(args)}`);
    });
  });

  it("keeps script trust confirmation modal and confirms successful trusted run", async () => {
    const user = userEvent.setup();
    renderWithProviders(<Launcher />);

    expect(await screen.findByText("Danger Script")).toBeInTheDocument();
    fireEvent.click(screen.getByText("Danger Script"));

    expect(
      await screen.findByText(/Run untrusted command\?|是否执行未信任命令？/),
    ).toBeInTheDocument();
    await user.click(
      screen.getByLabelText(
        /Trust this launcher item for future runs|信任此启动项，后续直接执行/,
      ),
    );
    await user.click(screen.getByRole("button", { name: /Launch|启动/ }));

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("launcher_set_trust", {
        payload: { itemId: "script-1", trusted: true },
      });
      expect(invokeMock).toHaveBeenCalledWith("launcher_execute", {
        payload: { type: "script", target: "rm -rf ./tmp" },
      });
    });
    expect(safeRecordMessageMock).toHaveBeenCalledWith(
      expect.objectContaining({
        severity: "success",
        dedupe_key: "launcher:execute:success:script-1",
      }),
    );
  });

  it("cancels untrusted script launch without executing", async () => {
    const user = userEvent.setup();
    renderWithProviders(<Launcher />);

    fireEvent.click(await screen.findByText("Danger Script"));
    await user.click(screen.getByRole("button", { name: /Cancel|取消/ }));

    await waitFor(() => {
      expect(
        screen.queryByText(/Run untrusted command\?|是否执行未信任命令？/),
      ).not.toBeInTheDocument();
    });
    expect(invokeMock).not.toHaveBeenCalledWith(
      "launcher_execute",
      expect.anything(),
    );
  });

  it("records failure when untrusted script execution fails", async () => {
    const user = userEvent.setup();
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "launcher_list") return { data: launcherItems };
      if (command === "ssh_tunnels_snapshot") return { groups: [], tunnels: [], runtime: [] };
      if (command === "protocol_router_status") {
        return { running: false, enabled: false, port: 0, route_count: 0 };
      }
      if (command === "get_storage_config") return {};
      if (command === "launcher_execute") throw new Error("permission denied");
      if (command === "launcher_mark_launched") return null;
      throw new Error(`Unhandled command: ${command}`);
    });

    renderWithProviders(<Launcher />);
    fireEvent.click(await screen.findByText("Danger Script"));
    await user.click(screen.getByRole("button", { name: /Launch|启动/ }));

    await waitFor(() => {
      expect(safeRecordMessageMock).toHaveBeenCalledWith(
        expect.objectContaining({
          severity: "error",
          dedupe_key: "launcher:execute:error:script-1",
        }),
      );
    });
  });
});
