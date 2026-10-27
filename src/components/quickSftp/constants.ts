import type { RemoteFileEntry } from "@/lib/sftp";

export const DEFAULT_BOOKMARKS = ["~", "/var/log", "/etc", "/tmp", "~/.onespace_trash"];
export const SFTP_RECENT_CONNECTIONS_KEY = "onespace:sftp-recent-connections";
export const MAX_RECENT_CONNECTIONS = 6;

export const INITIAL_MOCK_ENTRIES: RemoteFileEntry[] = [
  {
    name: "nginx",
    is_dir: true,
    size: 4096,
    permissions: "drwxr-xr-x",
    modified: "2026-10-21 14:00",
    path: "~/nginx",
  },
  {
    name: "app.log",
    is_dir: false,
    size: 102400,
    permissions: "-rw-r--r--",
    modified: "2026-10-21 15:30",
    path: "~/app.log",
  },
  {
    name: "config.json",
    is_dir: false,
    size: 1240,
    permissions: "-rw-r--r--",
    modified: "2026-10-20 09:12",
    path: "~/config.json",
  },
];
