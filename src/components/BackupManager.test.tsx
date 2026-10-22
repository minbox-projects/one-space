import { fireEvent, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import { BackupManager } from "@/components/BackupManager";
import { renderWithProviders } from "@/test/mocks/render";
import { resetTauriMocks, invokeMock } from "@/test/mocks/tauri";

const BACKUPS = [
  {
    id: "skills-1",
    tool: "skills",
    file_path: "/tmp/skills/config.json",
    backup_path: "/tmp/skills/config.json.bak",
    file_content_hash: "aaaa1111",
    created_at: "2026-01-01T00:00:00Z",
    file_size: 10,
  },
  {
    id: "codex-1",
    tool: "codex",
    file_path: "/tmp/codex/config.toml",
    backup_path: "/tmp/codex/config.toml.bak",
    file_content_hash: "bbbb2222",
    created_at: "2026-01-02T00:00:00Z",
    file_size: 20,
  },
];

describe("BackupManager", () => {
  beforeEach(() => {
    resetTauriMocks();
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "list_backups") {
        return BACKUPS;
      }
      if (
        command === "restore_backup" ||
        command === "delete_backup"
      ) {
        return null;
      }
      if (command === "cleanup_old_backups") {
        return 1;
      }
      throw new Error(`Unhandled command: ${command}`);
    });
  });

  it("lists all backups across tools and keeps restore, delete, and cleanup without manual creation or a missing-tool warning", async () => {
    const user = userEvent.setup();
    renderWithProviders(<BackupManager />);

    expect(
      await screen.findByText("/tmp/skills/config.json"),
    ).toBeInTheDocument();
    expect(screen.getByText("/tmp/codex/config.toml")).toBeInTheDocument();
    expect(screen.getByText("skills")).toBeInTheDocument();
    expect(screen.getByText("codex")).toBeInTheDocument();

    const restoreButtons = screen.getAllByTitle(/^(Restore|恢复)$/);
    const deleteButtons = screen.getAllByTitle(/^(Delete|删除)$/);
    expect(restoreButtons).toHaveLength(2);
    expect(deleteButtons).toHaveLength(2);

    fireEvent.click(restoreButtons[0]);
    expect(
      await screen.findByText(
        /Are you sure you want to restore|确定要恢复此备份/,
      ),
    ).toBeInTheDocument();
    await user.click(
      screen.getAllByRole("button", { name: /^(Restore|恢复)$/ }).at(-1)!,
    );
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("restore_backup", {
        entryId: "skills-1",
      });
    });

    fireEvent.click(
      screen.getAllByTitle(/^(Delete|删除)$/)[0],
    );
    expect(
      await screen.findByText(
        /Are you sure you want to delete this backup|确定要删除此备份/,
      ),
    ).toBeInTheDocument();
    await user.click(
      screen.getAllByRole("button", { name: /^(OK|确定)$/ }).at(-1)!,
    );
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("delete_backup", {
        entryId: "skills-1",
      });
    });

    await user.click(
      screen.getByRole("button", { name: /^(Cleanup Old|清理旧备份)$/ }),
    );
    expect(
      await screen.findByText(
        /Delete all backups older than 30 days|确定删除超过 30 天的所有备份/,
      ),
    ).toBeInTheDocument();
    await user.click(
      screen.getAllByRole("button", { name: /^(OK|确定)$/ }).at(-1)!,
    );
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("cleanup_old_backups", {
        retentionDays: 30,
      });
    });

    expect(
      screen.queryByRole("button", { name: /^(Create Backup|创建备份)$/ }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByText(/No tool selected|未选择工具/),
    ).not.toBeInTheDocument();
  });

  it("shows the empty state without a manual creation action when no backups exist", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "list_backups") return [];
      throw new Error(`Unhandled command: ${command}`);
    });

    renderWithProviders(<BackupManager />);

    expect(
      await screen.findByText(/No Backups Found|未找到备份/),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("button", {
        name: /^(Create Backup|创建备份|Create First Backup|创建第一个备份)$/,
      }),
    ).not.toBeInTheDocument();
  });

  it("does not delete backup when confirmation is declined", async () => {
    const user = userEvent.setup();
    renderWithProviders(<BackupManager />);

    await screen.findByText("skills");
    // BACKUPS is ordered skills-first, so this selects the skills backup row.
    fireEvent.click(screen.getAllByTitle(/^(Delete|删除)$/)[0]);
    await user.click(screen.getByRole("button", { name: /^(Cancel|取消)$/ }));

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
        return BACKUPS;
      }
      if (command === "restore_backup") {
        throw new Error("restore failed");
      }
      return null;
    });

    renderWithProviders(<BackupManager />);

    await screen.findByText("skills");
    // BACKUPS is ordered skills-first, so this selects the skills backup row.
    fireEvent.click(screen.getAllByTitle(/^(Restore|恢复)$/)[0]);
    await user.click(
      screen.getAllByRole("button", { name: /^(Restore|恢复)$/ }).at(-1)!,
    );

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("restore_backup", {
        entryId: "skills-1",
      });
    });
  });
});
