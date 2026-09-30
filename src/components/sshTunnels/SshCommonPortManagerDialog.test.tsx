import { act, fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { SshCommonPortManagerDialog } from "./SshCommonPortManagerDialog";
import type { SshCommonPortView } from "./types";

const mockPorts: SshCommonPortView[] = [
  {
    id: "port-1",
    name: "MySQL",
    localPort: 3306,
    remotePort: 3306,
    description: "MySQL database",
    created_at: 1,
    updated_at: 1,
  },
  {
    id: "port-2",
    name: "Redis",
    localPort: 6379,
    remotePort: 6379,
    description: "Redis store",
    created_at: 2,
    updated_at: 2,
  },
];

describe("SshCommonPortManagerDialog", () => {
  it("renders common ports list when opened", () => {
    render(
      <SshCommonPortManagerDialog
        open={true}
        onOpenChange={vi.fn()}
        ports={mockPorts}
        submitting={false}
        onCreate={vi.fn()}
        onUpdate={vi.fn()}
        onDelete={vi.fn()}
      />,
    );

    expect(screen.getByText("3306 → 3306")).toBeInTheDocument();
    expect(screen.getByText("MySQL")).toBeInTheDocument();
    expect(screen.getByText("6379 → 6379")).toBeInTheDocument();
    expect(screen.getByText("Redis")).toBeInTheDocument();
  });

  it("validates and calls onCreate when adding a port", async () => {
    const onCreate = vi.fn();
    render(
      <SshCommonPortManagerDialog
        open={true}
        onOpenChange={vi.fn()}
        ports={mockPorts}
        submitting={false}
        onCreate={onCreate}
        onUpdate={vi.fn()}
        onDelete={vi.fn()}
      />,
    );

    const localInput = screen.getByTitle(/Local Port|本地端口/i);
    const remoteInput = screen.getByTitle(/Target Port|目标端口/i);
    const nameInput = screen.getByPlaceholderText(/Service name|服务名称/i);
    const addButton = screen.getByRole("button", { name: /Add|添加|新增/i });

    // Try submit without values
    await act(async () => {
      fireEvent.click(addButton);
    });
    expect(onCreate).not.toHaveBeenCalled();

    // Fill valid values
    await act(async () => {
      fireEvent.change(localInput, { target: { value: "8080" } });
      fireEvent.change(remoteInput, { target: { value: "8080" } });
      fireEvent.change(nameInput, { target: { value: "HTTP-Alt" } });
      fireEvent.click(addButton);
    });

    expect(onCreate).toHaveBeenCalledWith({
      localPort: 8080,
      remotePort: 8080,
      name: "HTTP-Alt",
      description: undefined,
    });
  });

  it("handles edit and delete triggers", async () => {
    const onUpdate = vi.fn();
    const onDelete = vi.fn();
    render(
      <SshCommonPortManagerDialog
        open={true}
        onOpenChange={vi.fn()}
        ports={mockPorts}
        submitting={false}
        onCreate={vi.fn()}
        onUpdate={onUpdate}
        onDelete={onDelete}
      />,
    );

    const editButtons = screen.getAllByRole("button", { name: /Edit|编辑/i });
    expect(editButtons.length).toBeGreaterThan(0);
    await act(async () => {
      fireEvent.click(editButtons[0]);
    });

    const saveButton = screen.getByRole("button", { name: /Save|保存/i });
    expect(saveButton).toBeInTheDocument();
    await act(async () => {
      fireEvent.click(saveButton);
    });

    expect(onUpdate).toHaveBeenCalledWith(
      mockPorts[0],
      expect.objectContaining({
        id: "port-1",
        name: "MySQL",
        localPort: 3306,
        remotePort: 3306,
      }),
    );

    const deleteButtons = screen.getAllByRole("button", { name: /Delete|删除/i });
    expect(deleteButtons.length).toBeGreaterThan(0);
    await act(async () => {
      fireEvent.click(deleteButtons[0]);
    });
    expect(onDelete).toHaveBeenCalledWith(mockPorts[0]);
  });
});
