import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { TunnelConfigDialog } from "./TunnelConfigDialog";
import type { TunnelConfig } from "@/lib/portTunnel";

describe("TunnelConfigDialog", () => {
  it("a) FRP + HTTP sends frp_server_port 7000 when untouched", async () => {
    const user = userEvent.setup();
    const onSave = vi.fn().mockResolvedValue(undefined);
    render(
      <TunnelConfigDialog open onOpenChange={vi.fn()} config={null} onSave={onSave} />,
    );
    await user.click(screen.getByRole("button", { name: /Custom FRP/i }));
    await user.type(screen.getByPlaceholderText("e.g. Next.js App"), "My Tunnel");
    const localPort = screen.getByPlaceholderText("3000");
    await user.clear(localPort);
    await user.type(localPort, "3000");
    await user.type(screen.getByPlaceholderText("frp.mydomain.com"), "frp.example.com");
    await user.type(screen.getByPlaceholderText("app.mydomain.com"), "app.example.com");

    await user.click(screen.getByRole("button", { name: /save|保存/i }));

    await waitFor(() => expect(onSave).toHaveBeenCalled());
    const payload = onSave.mock.calls[0][0] as TunnelConfig;
    expect(payload.frp_server_port).toBe(7000);
  });

  it("b) FRP + HTTP without domain shows error and does not save", async () => {
    const user = userEvent.setup();
    const onSave = vi.fn().mockResolvedValue(undefined);
    render(
      <TunnelConfigDialog open onOpenChange={vi.fn()} config={null} onSave={onSave} />,
    );
    await user.click(screen.getByRole("button", { name: /Custom FRP/i }));
    await user.type(screen.getByPlaceholderText("frp.mydomain.com"), "frp.example.com");

    await user.click(screen.getByRole("button", { name: /save|保存/i }));

    await waitFor(() => expect(screen.getByText(/custom domain/i)).toBeInTheDocument());
    expect(onSave).not.toHaveBeenCalled();
  });

  it("c) FRP + TCP without remote port shows error and does not save", async () => {
    const user = userEvent.setup();
    const onSave = vi.fn().mockResolvedValue(undefined);
    render(
      <TunnelConfigDialog open onOpenChange={vi.fn()} config={null} onSave={onSave} />,
    );
    await user.click(screen.getByRole("button", { name: /Custom FRP/i }));
    await user.click(screen.getByRole("button", { name: "TCP" }));
    await user.type(screen.getByPlaceholderText("frp.mydomain.com"), "frp.example.com");

    await user.click(screen.getByRole("button", { name: /save|保存/i }));

    await waitFor(() => expect(screen.getByText(/remote port/i)).toBeInTheDocument());
    expect(onSave).not.toHaveBeenCalled();
  });

  it("d) FRP + TCP saves protocol, server port and remote port", async () => {
    const user = userEvent.setup();
    const onSave = vi.fn().mockResolvedValue(undefined);
    render(
      <TunnelConfigDialog open onOpenChange={vi.fn()} config={null} onSave={onSave} />,
    );
    await user.click(screen.getByRole("button", { name: /Custom FRP/i }));
    await user.click(screen.getByRole("button", { name: "TCP" }));
    await user.type(screen.getByPlaceholderText("e.g. Next.js App"), "My Tunnel");
    const localPort = screen.getByPlaceholderText("3000");
    await user.clear(localPort);
    await user.type(localPort, "8080");
    await user.type(screen.getByPlaceholderText("frp.mydomain.com"), "frp.example.com");
    await user.type(screen.getByPlaceholderText("Optional Token"), "tok");
    await user.type(screen.getByPlaceholderText("e.g. 8080"), "9000");
    const serverPort = screen.getByPlaceholderText("7000");
    await user.clear(serverPort);
    await user.type(serverPort, "23550");

    await user.click(screen.getByRole("button", { name: /save|保存/i }));

    await waitFor(() => expect(onSave).toHaveBeenCalled());
    const payload = onSave.mock.calls[0][0] as TunnelConfig;
    expect(payload.protocol).toBe("tcp");
    expect(payload.frp_server_port).toBe(23550);
    expect(payload.remote_port).toBe(9000);
  });
});
