import { fireEvent, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import { BackupManager } from "@/components/BackupManager";
import { renderWithProviders } from "@/test/mocks/render";
import { resetTauriMocks, invokeMock } from "@/test/mocks/tauri";

describe("BackupManager", () => {
  beforeEach(() => {
    resetTauriMocks();
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "list_backups") {
        return [
          {
            id: "b1",
            tool: "skills",
            file_path: "/tmp/a",
            backup_path: "/tmp/a.bak",
            file_content_hash: "hash",
            created_at: "2026-01-01T00:00:00Z",
            file_size: 10,
          },
        ];
      }
      if (
        command === "create_backup" ||
        command === "restore_backup" ||
        command === "delete_backup"
      ) {
        return null;
      }
      throw new Error(`Unhandled command: ${command}`);
    });
  });

  it("creates backup after confirmation and reloads list", async () => {
    const user = userEvent.setup();
    renderWithProviders(<BackupManager activeTool="skills" />);

    await screen.findByText("skills");
    await user.click(
      screen.getAllByRole("button", { name: /Create Backup|创建备份/ })[0],
    );
    await user.click(
      screen.getAllByRole("button", { name: /Create Backup|创建备份/ }).at(-1)!,
    );

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("create_backup", {
        tool: "skills",
        reason: "Manual backup",
      });
    });
  });

  it("does not delete backup when confirmation is declined", async () => {
    const user = userEvent.setup();
    renderWithProviders(<BackupManager activeTool="skills" />);

    await screen.findByText("skills");
    fireEvent.click(screen.getByTitle(/deleteBackup|删除/));
    await user.click(screen.getByRole("button", { name: /Cancel|取消/ }));

    await waitFor(() => {
      expect(invokeMock).not.toHaveBeenCalledWith(
        "delete_backup",
        expect.anything(),
      );
    });
    expect(invokeMock).not.toHaveBeenCalledWith(
      "delete_backup",
      expect.anything(),
    );
  });

  it("shows failure path when restore command rejects", async () => {
    const user = userEvent.setup();
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "list_backups") {
        return [
          {
            id: "b1",
            tool: "skills",
            file_path: "/tmp/a",
            backup_path: "/tmp/a.bak",
            file_content_hash: "hash",
            created_at: "2026-01-01T00:00:00Z",
            file_size: 10,
          },
        ];
      }
      if (command === "restore_backup") {
        throw new Error("restore failed");
      }
      return null;
    });

    renderWithProviders(<BackupManager activeTool="skills" />);

    await screen.findByText("skills");
    fireEvent.click(screen.getByTitle(/Restore|恢复/));
    await user.click(
      screen.getAllByRole("button", { name: /Restore|恢复/ }).at(-1)!,
    );

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("restore_backup", {
        entryId: "b1",
      });
    });
  });
});
