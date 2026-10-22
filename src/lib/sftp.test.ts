import { describe, expect, it, vi } from "vitest";
import {
  formatBytes,
  formatSpeed,
  formatEta,
  sftpGetDefaultDownloadDir,
  sftpDownloadFile,
  sftpUploadFile,
  sftpCancelTransfer,
  sftpDeleteEntry,
  sftpRestoreEntry,
  sftpListTrash,
  sftpRestoreTrashItem,
  sftpClearTrash,
} from "./sftp";
import { invoke } from "@tauri-apps/api/core";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

describe("src/lib/sftp", () => {
  describe("formatBytes", () => {
    it("formats 0 and negative bytes correctly", () => {
      expect(formatBytes(0)).toBe("0 B");
      expect(formatBytes(-10)).toBe("0 B");
    });

    it("formats various byte sizes", () => {
      expect(formatBytes(512)).toBe("512 B");
      expect(formatBytes(1024)).toBe("1.0 KB");
      expect(formatBytes(1024 * 1024 * 1.5)).toBe("1.5 MB");
      expect(formatBytes(1024 * 1024 * 1024 * 3.2)).toBe("3.2 GB");
    });
  });

  describe("formatSpeed", () => {
    it("formats network speed with per-second suffix", () => {
      expect(formatSpeed(0)).toBe("0 B/s");
      expect(formatSpeed(1024 * 1024 * 2.4)).toBe("2.4 MB/s");
    });
  });

  describe("formatEta", () => {
    it("handles null, undefined, and non-positive inputs", () => {
      expect(formatEta(null, true)).toBe("计算中...");
      expect(formatEta(undefined, false)).toBe("Calculating...");
      expect(formatEta(-1, true)).toBe("计算中...");
      expect(formatEta(0, true)).toBe("已完成");
      expect(formatEta(0, false)).toBe("Done");
    });

    it("formats seconds under one minute", () => {
      expect(formatEta(15, true)).toBe("剩余 15 秒");
      expect(formatEta(15, false)).toBe("15s left");
    });

    it("formats minutes and seconds", () => {
      expect(formatEta(60, true)).toBe("剩余 1 分钟");
      expect(formatEta(60, false)).toBe("1m left");
      expect(formatEta(125, true)).toBe("剩余 2 分 5 秒");
      expect(formatEta(125, false)).toBe("2m 5s left");
    });

    it("formats hours and minutes", () => {
      expect(formatEta(3600, true)).toBe("剩余 1 小时");
      expect(formatEta(3600, false)).toBe("1h left");
      expect(formatEta(3600 + 1200, true)).toBe("剩余 1 小时 20 分");
      expect(formatEta(3600 + 1200, false)).toBe("1h 20m left");
    });
  });

  describe("API invokers", () => {
    it("calls sftp_get_default_download_dir", async () => {
      vi.mocked(invoke).mockResolvedValueOnce("/mock/downloads");
      const res = await sftpGetDefaultDownloadDir();
      expect(invoke).toHaveBeenCalledWith("sftp_get_default_download_dir");
      expect(res).toBe("/mock/downloads");
    });

    it("calls sftp_download_file with proper args", async () => {
      vi.mocked(invoke).mockResolvedValueOnce(undefined);
      await sftpDownloadFile({
        taskId: "task-1",
        host: "my-host",
        remotePath: "/var/log/app.log",
        localPath: "/downloads/app.log",
        fileSize: 1024,
      });
      expect(invoke).toHaveBeenCalledWith("sftp_download_file", {
        taskId: "task-1",
        host: "my-host",
        remotePath: "/var/log/app.log",
        localPath: "/downloads/app.log",
        fileSize: 1024,
      });
    });

    it("calls sftp_upload_file with proper args", async () => {
      vi.mocked(invoke).mockResolvedValueOnce(undefined);
      await sftpUploadFile({
        taskId: "task-2",
        host: "my-host",
        localPath: "/local/data.json",
        remotePath: "/remote/data.json",
      });
      expect(invoke).toHaveBeenCalledWith("sftp_upload_file", {
        taskId: "task-2",
        host: "my-host",
        localPath: "/local/data.json",
        remotePath: "/remote/data.json",
      });
    });

    it("calls sftp_cancel_transfer with proper args", async () => {
      vi.mocked(invoke).mockResolvedValueOnce(undefined);
      await sftpCancelTransfer("task-3");
      expect(invoke).toHaveBeenCalledWith("sftp_cancel_transfer", {
        taskId: "task-3",
      });
    });

    it("calls sftp_delete_entry with proper args", async () => {
      const mockResult = {
        original_path: "/var/log/nginx.log",
        trash_path: "~/.onespace_trash/123_nginx.log",
        is_permanent: false,
      };
      vi.mocked(invoke).mockResolvedValueOnce(mockResult);
      const res = await sftpDeleteEntry("my-host", "/var/log/nginx.log", false);
      expect(invoke).toHaveBeenCalledWith("sftp_delete_entry", {
        host: "my-host",
        remotePath: "/var/log/nginx.log",
        permanent: false,
      });
      expect(res).toEqual(mockResult);
    });

    it("calls sftp_restore_entry with proper args", async () => {
      vi.mocked(invoke).mockResolvedValueOnce(undefined);
      await sftpRestoreEntry(
        "my-host",
        "~/.onespace_trash/123_nginx.log",
        "/var/log/nginx.log",
      );
      expect(invoke).toHaveBeenCalledWith("sftp_restore_entry", {
        host: "my-host",
        trashPath: "~/.onespace_trash/123_nginx.log",
        originalPath: "/var/log/nginx.log",
      });
    });

    it("calls sftp_list_trash with proper args", async () => {
      const mockItems = [
        {
          id: "123_nginx.log",
          name: "nginx.log",
          original_path: "/var/log/nginx.log",
          trash_path: "~/.onespace_trash/123_nginx.log",
          size: 1024,
          is_dir: false,
          deleted_at: "1729587400",
        },
      ];
      vi.mocked(invoke).mockResolvedValueOnce(mockItems);
      const res = await sftpListTrash("my-host");
      expect(invoke).toHaveBeenCalledWith("sftp_list_trash", {
        host: "my-host",
      });
      expect(res).toEqual(mockItems);
    });

    it("calls sftp_restore_trash_item with proper args", async () => {
      vi.mocked(invoke).mockResolvedValueOnce("/var/log/nginx.log");
      const res = await sftpRestoreTrashItem("my-host", "123_nginx.log");
      expect(invoke).toHaveBeenCalledWith("sftp_restore_trash_item", {
        host: "my-host",
        trashName: "123_nginx.log",
      });
      expect(res).toBe("/var/log/nginx.log");
    });

    it("calls sftp_clear_trash with proper args", async () => {
      vi.mocked(invoke).mockResolvedValueOnce(undefined);
      await sftpClearTrash("my-host");
      expect(invoke).toHaveBeenCalledWith("sftp_clear_trash", {
        host: "my-host",
      });
    });
  });
});
