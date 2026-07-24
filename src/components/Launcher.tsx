import { useEffect, useMemo, useState, useCallback, useRef } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import { emit, listen } from "@tauri-apps/api/event";
import { open, save } from "@tauri-apps/plugin-dialog";
import {
  deriveSshTunnelLauncherSummary,
  type LauncherSshTunnelSummary,
} from "../lib/sshTunnelSummary";
import {
  protocolRouterStatus,
  type ProtocolRouterStatus,
} from "@/lib/protocolRouter";
import {
  Rocket,
  Plus,
  Trash2,
  Command,
  Globe,
  FolderOpen,
  Search,
  Server,
  Star,
  Pin,
  PinOff,
  ArrowUp,
  ArrowDown,
  Edit,
  Upload,
  Download,
  ShieldAlert,
  Workflow,
  Waypoints,
  Loader2,
  Route,
  Cloud,
  KeyRound,
  Braces,
} from "lucide-react";
import { useConfirmDialog } from "./ConfirmDialogProvider";
import { useToast } from "./ToastProvider";
import type { SshTunnelsSnapshot } from "./sshTunnels/types";
import { errorToMessage, safeRecordMessage } from "@/lib/messages";
import { runUserAction } from "@/lib/userActions";
import {
  LAUNCHER_TOOL_VISIBILITY_UPDATED_EVENT,
  readLauncherToolVisibility,
} from "@/lib/launcherToolVisibility";

interface LauncherItem {
  id: string;
  name: string;
  type: "app" | "script" | "url" | "folder" | "internal";
  target: string;
  pinned: boolean;
  pin_order: number;
  launch_count: number;
  last_launched_at?: number;
  trusted: boolean;
  created_at: number;
  updated_at: number;
}

interface ApiResp<T> {
  ok: boolean;
  data: T;
  meta: { schema_version: number; revision: number };
}

interface LauncherItemInput {
  id?: string;
  name: string;
  type: LauncherItem["type"];
  target: string;
  pinned?: boolean;
  pin_order?: number;
  trusted?: boolean;
}

interface LegacyLauncherItem {
  id?: string;
  name: string;
  command: string;
  type: "app" | "script" | "url" | "folder";
}

const MIGRATION_MARKER = "onespace_launcher_migrated_v1";
const SEEDED_MARKER = "onespace_launcher_seeded_v1";
const LEGACY_STORAGE_KEY = "onespace_launcher_items";

const DEFAULT_LAUNCHER_ITEMS: LauncherItemInput[] = [
  { name: "VS Code", type: "app", target: 'open -a "Visual Studio Code"' },
  { name: "Google Chrome", type: "app", target: 'open -a "Google Chrome"' },
  { name: "System Settings", type: "app", target: 'open -a "System Settings"' },
];

const INTERNAL_TARGETS: Array<{
  id: string;
  labelKey: string;
  fallback: string;
}> = [
  { id: "launcher", labelKey: "launcher", fallback: "Launcher" },
  {
    id: "ai-sessions",
    labelKey: "aiSessions",
    fallback: "AI Terminal Sessions",
  },
  {
    id: "ai-assistants",
    labelKey: "aiAssistants",
    fallback: "AI Workspace",
  },
  {
    id: "ai-environments",
    labelKey: "aiEnvironments",
    fallback: "AI Environments",
  },
  { id: "skills", labelKey: "skills", fallback: "Skills" },
  { id: "mcp-servers", labelKey: "mcpServers", fallback: "MCP Servers" },
  { id: "ssh", labelKey: "sshServers", fallback: "SSH Servers" },
  { id: "ssh-tunnels", labelKey: "sshTunnels", fallback: "SSH Tunnels" },
  { id: "protocol-router", labelKey: "protocolRouter", fallback: "Protocol Router" },
  { id: "snippets", labelKey: "snippets", fallback: "Snippets" },
  { id: "bookmarks", labelKey: "bookmarks", fallback: "Bookmarks" },
  { id: "notes", labelKey: "notes", fallback: "Notes" },
  { id: "mail", labelKey: "mail", fallback: "Mail" },
  { id: "settings", labelKey: "settings", fallback: "Settings" },
  { id: "documentation", labelKey: "usageDocs", fallback: "Documentation" },
];

async function safelyUnlisten(
  label: string,
  unlisten: () => void | Promise<void>,
) {
  try {
    await Promise.resolve(unlisten());
  } catch (error) {
    console.warn(`Failed to unlisten ${label}`, error);
  }
}
const LAUNCHER_TYPE_ORDER: LauncherItem["type"][] = [
  "app",
  "script",
  "url",
  "folder",
  "internal",
];

function sortLauncherItems(items: LauncherItem[]) {
  return [...items].sort((a, b) => {
    if (a.pinned !== b.pinned) return a.pinned ? -1 : 1;
    if (a.pinned && b.pinned) return a.pin_order - b.pin_order;
    return (b.last_launched_at || 0) - (a.last_launched_at || 0);
  });
}

function launcherIcon(type: LauncherItem["type"]) {
  if (type === "url") return Globe;
  if (type === "folder") return FolderOpen;
  if (type === "script") return Command;
  if (type === "internal") return Workflow;
  return Rocket;
}

function formatInvokeError(err: unknown): string {
  if (typeof err === "string") return err;
  if (err && typeof err === "object") {
    const maybe = err as { message?: unknown; error?: unknown };
    if (typeof maybe.message === "string" && maybe.message.trim())
      return maybe.message;
    if (typeof maybe.error === "string" && maybe.error.trim())
      return maybe.error;
    try {
      return JSON.stringify(err);
    } catch (_e) {
      return String(err);
    }
  }
  return String(err);
}

function isSshTunnelsSnapshot(payload: unknown): payload is SshTunnelsSnapshot {
  if (!payload || typeof payload !== "object") return false;
  const snapshot = payload as Partial<SshTunnelsSnapshot>;
  return (
    Array.isArray(snapshot.groups) &&
    Array.isArray(snapshot.tunnels) &&
    Array.isArray(snapshot.runtime)
  );
}

type LauncherWindowBindings = typeof window & {
  setActiveTab?: (tab: string) => void;
  setSettingsTab?: (tab: string) => void;
};

export function Launcher({ isVisible = true }: { isVisible?: boolean }) {
  const { t, i18n } = useTranslation();
  const confirmDialog = useConfirmDialog();
  const { pushToast } = useToast();
  const [items, setItems] = useState<LauncherItem[]>([]);
  const [loading, setLoading] = useState(true);
  const [searchTerm, setSearchTerm] = useState("");

  const [isEditing, setIsEditing] = useState(false);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [nameInput, setNameInput] = useState("");
  const [typeInput, setTypeInput] = useState<LauncherItem["type"]>("app");
  const [targetInput, setTargetInput] = useState("");
  const [pinnedInput, setPinnedInput] = useState(false);

  const [pendingScriptItem, setPendingScriptItem] =
    useState<LauncherItem | null>(null);
  const [trustOnConfirm, setTrustOnConfirm] = useState(false);
  const [appIconCache, setAppIconCache] = useState<
    Record<string, string | null>
  >({});
  const [sshTunnelSummary, setSshTunnelSummary] =
    useState<LauncherSshTunnelSummary | null>(null);
  const [protocolRouterStatusState, setProtocolRouterStatusState] =
    useState<ProtocolRouterStatus | null>(null);
  const [toolVisibility, setToolVisibility] = useState(
    readLauncherToolVisibility,
  );
  const sshTunnelSummaryVersionRef = useRef(0);

  const isTauri = "__TAURI_INTERNALS__" in window;
  const appIconCacheKey = (target: string) => target.trim().toLowerCase();
  const actionContext = useMemo(
    () => ({
      t,
      confirm: confirmDialog,
      pushToast,
      recordMessage: safeRecordMessage,
    }),
    [confirmDialog, pushToast, t],
  );

  const applySshTunnelSummary = useCallback(
    (snapshot: SshTunnelsSnapshot, _source: string, version: number) => {
      if (version !== sshTunnelSummaryVersionRef.current) {
        return;
      }
      const summary = deriveSshTunnelLauncherSummary(snapshot);
      setSshTunnelSummary(summary);
    },
    [],
  );

  const loadSshTunnelSummary = useCallback(async () => {
    if (!isTauri) return;
    const version = sshTunnelSummaryVersionRef.current + 1;
    sshTunnelSummaryVersionRef.current = version;
    try {
      const snapshot = await invoke<SshTunnelsSnapshot>("ssh_tunnels_snapshot");
      applySshTunnelSummary(snapshot, `load#${version}`, version);
    } catch (err) {
      if (version !== sshTunnelSummaryVersionRef.current) {
        return;
      }
      console.error("Failed to load SSH tunnel launcher summary", err);
      setSshTunnelSummary(null);
    }
  }, [applySshTunnelSummary, isTauri]);

  const loadProtocolRouterStatus = useCallback(async () => {
    if (!isTauri) return;
    try {
      setProtocolRouterStatusState(await protocolRouterStatus());
    } catch (err) {
      console.error("Failed to load protocol router launcher status", err);
      setProtocolRouterStatusState(null);
    }
  }, [isTauri]);

  const openInternalTarget = useCallback((target: string) => {
    const appWindow = window as LauncherWindowBindings;
    appWindow.setActiveTab?.(target);
  }, []);

  useEffect(() => {
    const refreshToolVisibility = () => {
      setToolVisibility(readLauncherToolVisibility());
    };
    window.addEventListener(
      LAUNCHER_TOOL_VISIBILITY_UPDATED_EVENT,
      refreshToolVisibility,
    );
    return () => {
      window.removeEventListener(
        LAUNCHER_TOOL_VISIBILITY_UPDATED_EVENT,
        refreshToolVisibility,
      );
    };
  }, []);

  const sortedItems = useMemo(() => sortLauncherItems(items), [items]);

  const filteredItems = useMemo(() => {
    const term = searchTerm.trim().toLowerCase();
    if (!term) return sortedItems;
    return sortedItems.filter((item) => {
      const title = item.name.toLowerCase();
      const target = item.target.toLowerCase();
      return title.includes(term) || target.includes(term);
    });
  }, [searchTerm, sortedItems]);

  const groupedItems = useMemo(
    () =>
      LAUNCHER_TYPE_ORDER.map((type) => ({
        type,
        items: filteredItems.filter((item) => item.type === type),
      })).filter((group) => group.items.length > 0),
    [filteredItems],
  );

  const formatRelativeTime = (ts?: number) => {
    if (!ts) return "";
    const nowMs = Date.now();
    const diffSec = Math.floor((nowMs - ts * 1000) / 1000);
    if (diffSec < 60) return t("launcherTimeJustNow", "just now");
    if (diffSec < 3600) {
      const value = Math.floor(diffSec / 60);
      return t("launcherTimeMinutesAgo", {
        value,
        defaultValue: `${value}m ago`,
      });
    }
    if (diffSec < 86400) {
      const value = Math.floor(diffSec / 3600);
      return t("launcherTimeHoursAgo", {
        value,
        defaultValue: `${value}h ago`,
      });
    }
    const value = Math.floor(diffSec / 86400);
    return t("launcherTimeDaysAgo", { value, defaultValue: `${value}d ago` });
  };

  useEffect(() => {
    if (!isTauri) return;

    const appTargets = Array.from(
      new Set(
        items
          .filter((item) => item.type === "app")
          .map((item) => item.target.trim())
          .filter((target) => target.length > 0),
      ),
    );
    const missingTargets = appTargets.filter(
      (target) => !(appIconCacheKey(target) in appIconCache),
    );
    if (missingTargets.length === 0) return;

    let cancelled = false;
    (async () => {
      const resolved = await Promise.all(
        missingTargets.map(async (target) => {
          try {
            const resp = await invoke<ApiResp<{ data_url?: string | null }>>(
              "launcher_resolve_app_icon",
              { target },
            );
            const dataUrl = resp.data?.data_url;
            const validDataUrl =
              typeof dataUrl === "string" && dataUrl.startsWith("data:image/")
                ? dataUrl
                : null;
            return [appIconCacheKey(target), validDataUrl] as const;
          } catch (_err) {
            return [appIconCacheKey(target), null] as const;
          }
        }),
      );

      if (cancelled) return;
      setAppIconCache((prev) => {
        const next = { ...prev };
        for (const [key, value] of resolved) {
          if (!(key in next)) {
            next[key] = value;
          } else if (next[key] == null && value) {
            next[key] = value;
          }
        }
        return next;
      });
    })();

    return () => {
      cancelled = true;
    };
  }, [items, isTauri, appIconCache]);

  useEffect(() => {
    if (!isTauri) return;

    let disposed = false;
    let teardown: (() => void) | null = null;

    void loadSshTunnelSummary();

    listen<SshTunnelsSnapshot | null>("ssh-tunnels-updated", (event) => {
      if (disposed) return;
      if (isSshTunnelsSnapshot(event.payload)) {
        const version = sshTunnelSummaryVersionRef.current + 1;
        sshTunnelSummaryVersionRef.current = version;
        applySshTunnelSummary(event.payload, `event#${version}`, version);
        return;
      }
      void loadSshTunnelSummary();
    })
      .then((unlisten) => {
        if (disposed) {
          void safelyUnlisten("ssh-tunnels-updated", unlisten);
          return;
        }
        teardown = unlisten;
        void loadSshTunnelSummary();
      })
      .catch((err) => {
        console.error("Failed to subscribe to ssh-tunnels-updated", err);
      });

    return () => {
      disposed = true;
      if (teardown) {
        const currentTeardown = teardown;
        teardown = null;
        void safelyUnlisten("ssh-tunnels-updated", currentTeardown);
      }
    };
  }, [applySshTunnelSummary, isTauri, loadSshTunnelSummary]);

  useEffect(() => {
    if (!isTauri || !isVisible) return;
    void loadSshTunnelSummary();
  }, [isTauri, isVisible, loadSshTunnelSummary]);

  useEffect(() => {
    if (!isTauri) return;

    let disposed = false;
    let teardown: (() => void) | null = null;

    void loadProtocolRouterStatus();

    listen("protocol-router-status-update", () => {
      if (disposed) return;
      void loadProtocolRouterStatus();
    })
      .then((unlisten) => {
        if (disposed) {
          void safelyUnlisten("protocol-router-status-update", unlisten);
          return;
        }
        teardown = unlisten;
      })
      .catch((err) => {
        console.error("Failed to subscribe to protocol-router-status-update", err);
      });

    return () => {
      disposed = true;
      if (teardown) {
        const currentTeardown = teardown;
        teardown = null;
        void safelyUnlisten("protocol-router-status-update", currentTeardown);
      }
    };
  }, [isTauri, loadProtocolRouterStatus]);

  useEffect(() => {
    if (!isTauri || !isVisible) return;
    void loadProtocolRouterStatus();
  }, [isTauri, isVisible, loadProtocolRouterStatus]);

  const typeLabelMap: Record<LauncherItem["type"], string> = {
    app: t("macApp", "Mac Application (open -a)"),
    script: t("shellCommand", "Shell Command"),
    url: t("websiteUrl", "Website URL"),
    folder: t("localFolder", "Local Folder"),
    internal: t("internalAction", "Internal Action"),
  };

  const pinnedOrderIds = useMemo(
    () => sortedItems.filter((item) => item.pinned).map((item) => item.id),
    [sortedItems],
  );
  const smartWorkspaceLabel =
    i18n.language === "zh" ? "AI 工作台" : "AI Workspace";

  const renderSshTunnelStatus = (summary?: LauncherSshTunnelSummary | null) => {
    if (!summary) return null;

    if (summary.state === "failed") {
      const label = t("launcherSshTunnelFailedAria", {
        count: summary.autoConnectFailedCount,
        defaultValue: `${summary.autoConnectFailedCount} auto-connect SSH tunnels failed`,
      });
      return (
        <span
          className="inline-flex items-center gap-1.5 rounded-full border border-destructive/20 bg-destructive/10 px-2.5 py-1 text-[11px] font-medium text-destructive"
          aria-label={label}
          title={label}
        >
          <span className="h-2 w-2 rounded-full bg-destructive" />
          {summary.autoConnectFailedCount}
        </span>
      );
    }

    if (summary.state === "connecting") {
      const label = t(
        "launcherSshTunnelConnectingAria",
        "SSH tunnels are connecting automatically.",
      );
      return (
        <span
          className="inline-flex items-center gap-1.5 rounded-full border border-amber-500/20 bg-amber-500/10 px-2.5 py-1 text-[11px] font-medium text-amber-600"
          aria-label={label}
          title={label}
        >
          <Loader2 className="h-3.5 w-3.5 animate-spin" />
          {t("launcherSshTunnelConnecting", "Connecting...")}
        </span>
      );
    }

    const label = t("launcherSshTunnelConnectedAria", {
      count: summary.connectedCount,
      defaultValue: `${summary.connectedCount} SSH tunnels connected`,
    });
    return (
      <span
        className="inline-flex items-center gap-1.5 rounded-full border border-emerald-500/20 bg-emerald-500/10 px-2.5 py-1 text-[11px] font-medium text-emerald-600"
        aria-label={label}
        title={label}
      >
        <span className="h-2 w-2 rounded-full bg-emerald-500" />
        {summary.connectedCount}
      </span>
    );
  };

  const renderProtocolRouterStatus = (status?: ProtocolRouterStatus | null) => {
    if (!status) return null;

    if (status.running) {
      const label = t("launcherProtocolRouterRunningAria", {
        port: status.port,
        routes: status.route_count,
        defaultValue: `Protocol router running on port ${status.port} with ${status.route_count} route(s)`,
      });
      return (
        <span
          className="inline-flex items-center gap-1.5 rounded-full border border-emerald-500/20 bg-emerald-500/10 px-2.5 py-1 text-[11px] font-medium text-emerald-600"
          aria-label={label}
          title={label}
        >
          <span className="h-2 w-2 rounded-full bg-emerald-500" />
          {t("launcherProtocolRouterRunning", "Running")}
        </span>
      );
    }

    const label = status.enabled
      ? t("launcherProtocolRouterStoppedAria", {
          port: status.port,
          defaultValue: `Protocol router is enabled but stopped on port ${status.port}`,
        })
      : t("launcherProtocolRouterDisabledAria", "Protocol router is disabled");
    return (
      <span
        className="inline-flex items-center gap-1.5 rounded-full border border-muted-foreground/20 bg-muted px-2.5 py-1 text-[11px] font-medium text-muted-foreground"
        aria-label={label}
        title={label}
      >
        <span className="h-2 w-2 rounded-full bg-muted-foreground" />
        {status.enabled
          ? t("launcherProtocolRouterStopped", "Stopped")
          : t("launcherProtocolRouterDisabled", "Disabled")}
      </span>
    );
  };

  const quickInternalTools = useMemo(() => {
    const allTools = [
      {
        id: "quick-bookmarks",
        name: t("bookmarks", "Bookmarks"),
        description: t(
          "launcherBookmarksDesc",
          "Save the links and resources you revisit often.",
        ),
        target: "bookmarks",
        icon: Star,
        visible: toolVisibility.bookmarks,
      },
      {
        id: "quick-cloud",
        name: t("cloud", "Cloud Drive"),
        description: t(
          "launcherCloudDriveDesc",
          "Browse and organize synced cloud files.",
        ),
        target: "cloud",
        icon: Cloud,
        visible: toolVisibility.cloud,
      },
      {
        id: "quick-ssh",
        name: t("sshServers", "SSH Servers"),
        description:
          t(
            "launcherSshServersDesc",
            "Open saved SSH hosts, history, and custom connections quickly.",
          ),
        target: "ssh",
        icon: Server,
        visible: toolVisibility.ssh,
      },
      {
        id: "quick-ssh-tunnels",
        name: t("sshTunnels", "SSH Tunnels"),
        description:
          t(
            "launcherSshTunnelsDesc",
            "Manage local, remote, and dynamic SOCKS5 SSH tunnels with built-in connectivity checks.",
          ),
        target: "ssh-tunnels",
        icon: Waypoints,
        statusBadge: renderSshTunnelStatus(sshTunnelSummary),
        visible: toolVisibility["ssh-tunnels"],
      },
      {
        id: "quick-protocol-router",
        name: t("protocolRouter", "Protocol Router"),
        description: t(
          "launcherProtocolRouterDesc",
          "Expose local Anthropic-compatible routes for Claude profiles and OpenAI-compatible providers.",
        ),
        target: "protocol-router",
        icon: Route,
        statusBadge: renderProtocolRouterStatus(protocolRouterStatusState),
        visible: toolVisibility["protocol-router"],
      },
      {
        id: "quick-random-password",
        name: t("randomPassword", "Random Password"),
        description: t(
          "randomPasswordToolDesc",
          "Generate passwords locally with the character groups you need.",
        ),
        target: "random-password",
        icon: KeyRound,
        visible: toolVisibility["random-password"],
      },
      {
        id: "quick-json-parser",
        name: t("jsonParser", "JSON Parser"),
        description: t(
          "jsonParserToolDesc",
          "Validate and format JSON locally in one editable workspace.",
        ),
        target: "json-parser",
        icon: Braces,
        visible: toolVisibility["json-parser"],
      },
    ];

    const visibleItems = allTools.filter((item) => item.visible);

    const term = searchTerm.trim().toLowerCase();
    if (!term) return visibleItems;
    return visibleItems.filter((item) =>
      `${item.name} ${item.description} ${item.target}`
        .toLowerCase()
        .includes(term),
    );
  }, [protocolRouterStatusState, searchTerm, sshTunnelSummary, t, toolVisibility]);

  const listLauncherItems = async (): Promise<LauncherItem[]> => {
    const resp = await invoke<ApiResp<LauncherItem[]>>("launcher_list");
    return resp.data || [];
  };

  const refreshLauncherItems = async () => {
    if (!isTauri) return;
    const loaded = await listLauncherItems();
    setItems(sortLauncherItems(loaded));
  };

  const upsertLauncherItem = async (item: LauncherItemInput) => {
    await invoke("launcher_upsert", { item });
  };

  const migrateLegacyLauncherIfNeeded = async () => {
    if (localStorage.getItem(MIGRATION_MARKER) === "1") return false;
    const raw = localStorage.getItem(LEGACY_STORAGE_KEY);
    if (!raw) {
      localStorage.setItem(MIGRATION_MARKER, "1");
      return false;
    }

    let parsed: LegacyLauncherItem[] = [];
    try {
      parsed = JSON.parse(raw) as LegacyLauncherItem[];
    } catch (_err) {
      localStorage.setItem(MIGRATION_MARKER, "1");
      return false;
    }

    if (!Array.isArray(parsed) || parsed.length === 0) {
      localStorage.setItem(MIGRATION_MARKER, "1");
      return false;
    }

    for (const item of parsed) {
      await upsertLauncherItem({
        id: item.id,
        name: item.name,
        type: item.type,
        target: item.command,
      });
    }

    localStorage.setItem(MIGRATION_MARKER, "1");
    localStorage.removeItem(LEGACY_STORAGE_KEY);
    return true;
  };

  const seedDefaultLauncherIfNeeded = async () => {
    if (localStorage.getItem(SEEDED_MARKER) === "1") return false;
    for (const item of DEFAULT_LAUNCHER_ITEMS) {
      await upsertLauncherItem(item);
    }
    localStorage.setItem(SEEDED_MARKER, "1");
    return true;
  };

  const bootstrap = async () => {
    if (!isTauri) {
      setLoading(false);
      return;
    }
    setLoading(true);
    try {
      let loaded = await listLauncherItems();
      if (loaded.length === 0) {
        const migrated = await migrateLegacyLauncherIfNeeded();
        if (migrated) {
          loaded = await listLauncherItems();
        } else {
          const seeded = await seedDefaultLauncherIfNeeded();
          if (seeded) {
            loaded = await listLauncherItems();
          }
        }
      }
      setItems(sortLauncherItems(loaded));
      emit("refresh-counts").catch(() => {});
    } catch (err) {
      console.error(err);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    bootstrap();
  }, []);

  const resetEditor = () => {
    setIsEditing(false);
    setEditingId(null);
    setNameInput("");
    setTypeInput("app");
    setTargetInput("");
    setPinnedInput(false);
  };

  const startCreate = () => {
    setIsEditing(true);
    setEditingId(null);
    setNameInput("");
    setTypeInput("app");
    setTargetInput("");
    setPinnedInput(false);
  };

  const startEdit = (item: LauncherItem) => {
    setIsEditing(true);
    setEditingId(item.id);
    setNameInput(item.name);
    setTypeInput(item.type);
    setTargetInput(item.target);
    setPinnedInput(item.pinned);
  };

  const handleSave = async () => {
    const name = nameInput.trim();
    const target = targetInput.trim();
    if (!name || !target) return;

    try {
      await upsertLauncherItem({
        id: editingId || undefined,
        name,
        type: typeInput,
        target,
        pinned: pinnedInput,
      });
      await refreshLauncherItems();
      emit("refresh-counts").catch(() => {});
      resetEditor();
    } catch (err) {
      console.error(err);
      pushToast({
        title: t("failedToSave", "Failed to save. Check console."),
        description: formatInvokeError(err),
        kind: "error",
      });
    }
  };

  const handleDelete = async (item: LauncherItem, e: React.MouseEvent) => {
    e.stopPropagation();
    try {
      const result = await runUserAction(
        actionContext,
        {
          source: "launcher",
          category: "delete",
          action: "delete-item",
          target: { tab: "launcher", entity_id: item.id },
          dedupeKey: `launcher:delete:${item.id}`,
          metadata: { item_type: item.type, target: item.target },
          confirm: {
            message: t("confirmDelete", { name: item.name }),
            title: t("confirmDeleteTitle", "Delete item"),
            okLabel: t("delete", "Delete"),
            cancelLabel: t("cancel", "Cancel"),
            kind: "error",
          },
          success: {
            title: t("launcherDeleteSuccessMessageTitle", "Launcher item deleted"),
            summary: t("launcherDeleteSuccessSummary", {
              name: item.name,
              defaultValue: "{{name}} was removed from Launcher.",
            }),
            toastTitle: t("deleteSuccess", "Deleted successfully"),
          },
          error: {
            title: t("launcherDeleteFailedTitle", "Failed to delete launcher item"),
            summary: t("launcherDeleteFailedSummary", {
              name: item.name,
              defaultValue: "Could not delete {{name}}.",
            }),
          },
        },
        () => invoke("launcher_delete", { payload: { itemId: item.id } }),
      );
      if (result === null) return;
      await refreshLauncherItems();
      emit("refresh-counts").catch(() => {});
    } catch (err) {
      console.error(err);
    }
  };

  const handleTogglePin = async (item: LauncherItem, e: React.MouseEvent) => {
    e.stopPropagation();
    const previousItems = items;
    const nextPinned = !item.pinned;
    setItems(
      sortLauncherItems(
        items.map((it) =>
          it.id === item.id
            ? {
                ...it,
                pinned: nextPinned,
                updated_at: Math.floor(Date.now() / 1000),
              }
            : it,
        ),
      ),
    );
    try {
      await upsertLauncherItem({
        id: item.id,
        name: item.name,
        type: item.type,
        target: item.target,
        pinned: nextPinned,
      });
      await refreshLauncherItems();
    } catch (err) {
      console.error(err);
      setItems(previousItems);
      pushToast({
        title: t("pinFailed", { error: formatInvokeError(err) }),
        kind: "error",
      });
    }
  };

  const handleMovePinned = async (
    itemId: string,
    direction: "up" | "down",
    e: React.MouseEvent,
  ) => {
    e.stopPropagation();
    const current = [...pinnedOrderIds];
    const idx = current.findIndex((id) => id === itemId);
    if (idx < 0) return;
    const swapWith = direction === "up" ? idx - 1 : idx + 1;
    if (swapWith < 0 || swapWith >= current.length) return;

    const next = [...current];
    [next[idx], next[swapWith]] = [next[swapWith], next[idx]];

    try {
      await invoke("launcher_reorder", { ids: next });
      await refreshLauncherItems();
    } catch (err) {
      console.error(err);
    }
  };

  const executeLaunch = async (item: LauncherItem) => {
    if (item.type === "internal") {
      openInternalTarget(item.target);
      await invoke("launcher_mark_launched", {
        payload: { itemId: item.id },
      }).catch(() => {});
      await refreshLauncherItems();
      return;
    }

    await invoke("launcher_execute", {
      payload: {
        type: item.type,
        target: item.target,
      },
    });
    await invoke("launcher_mark_launched", {
      payload: { itemId: item.id },
    }).catch(() => {});
    await refreshLauncherItems();
    emit("refresh-counts").catch(() => {});
  };

  const handleLaunch = async (item: LauncherItem) => {
    if (!isTauri) return;

    if (item.type === "script" && !item.trusted) {
      setPendingScriptItem(item);
      setTrustOnConfirm(false);
      return;
    }

    try {
      await executeLaunch(item);
    } catch (err) {
      console.error(err);
      const detail = errorToMessage(err);
      void safeRecordMessage({
        source: "launcher",
        category: "execute",
        severity: "error",
        title: t("launcherLaunchFailedMessageTitle", "Launcher failed to start"),
        summary: `${item.name}: ${detail.split("\n").find(Boolean) || "Launch failed"}`,
        detail,
        dedupe_key: `launcher:execute:error:${item.id}`,
        target: { tab: "launcher", entity_id: item.id },
        metadata: { item_type: item.type, target: item.target },
      });
      pushToast({
        title: t("failedToLaunch", "Failed to launch. Check console."),
        description: formatInvokeError(err),
        kind: "error",
      });
    }
  };

  const confirmScriptLaunch = async () => {
    const item = pendingScriptItem;
    if (!item) return;

    setPendingScriptItem(null);
    try {
      if (trustOnConfirm) {
        await invoke("launcher_set_trust", {
          payload: { itemId: item.id, trusted: true },
        });
      }
      await executeLaunch(item);
      await safeRecordMessage({
        source: "launcher",
        category: "execute",
        severity: "success",
        title: t("launcherLaunchSuccessMessageTitle", "Launcher action started"),
        summary: t("launcherLaunchSuccessSummary", {
          name: item.name,
          defaultValue: "{{name}} started successfully.",
        }),
        dedupe_key: `launcher:execute:success:${item.id}`,
        target: { tab: "launcher", entity_id: item.id },
        metadata: { item_type: item.type, target: item.target },
      });
      pushToast({
        title: t("launcherLaunchStarted", "Launch started"),
        description: item.name,
        kind: "success",
      });
    } catch (err) {
      console.error(err);
      const detail = errorToMessage(err);
      void safeRecordMessage({
        source: "launcher",
        category: "execute",
        severity: "error",
        title: t("launcherLaunchFailedMessageTitle", "Launcher failed to start"),
        summary: `${item.name}: ${detail.split("\n").find(Boolean) || "Launch failed"}`,
        detail,
        dedupe_key: `launcher:execute:error:${item.id}`,
        target: { tab: "launcher", entity_id: item.id },
        metadata: { item_type: item.type, target: item.target },
      });
      pushToast({
        title: t("failedToLaunch", "Failed to launch. Check console."),
        description: formatInvokeError(err),
        kind: "error",
      });
    } finally {
      setTrustOnConfirm(false);
    }
  };

  const cancelScriptLaunch = () => {
    setPendingScriptItem(null);
    setTrustOnConfirm(false);
  };

  const handleExport = async () => {
    if (!isTauri) return;
    try {
      const stamp = new Date().toISOString().replace(/[:.]/g, "-");
      const outputPath = await save({
        defaultPath: `onespace-launcher-export-${stamp}.json`,
        filters: [{ name: "JSON", extensions: ["json"] }],
      });
      if (!outputPath || Array.isArray(outputPath)) return;
      await runUserAction(
        actionContext,
        {
          source: "launcher",
          category: "export",
          action: "export-items",
          target: { tab: "launcher" },
          dedupeKey: "launcher:export",
          metadata: { output_path: outputPath },
          confirm: {
            message: t(
              "launcherExportConfirm",
              "Export Launcher items to the selected JSON file?",
            ),
            title: t("launcherExportConfirmTitle", "Export Launcher items"),
            okLabel: t("export", "Export"),
            cancelLabel: t("cancel", "Cancel"),
            kind: "warning",
          },
          success: {
            title: t("launcherExportedMessageTitle", "Launcher items exported"),
            summary: t("exportedTo", { path: outputPath }),
            toastTitle: t("launcherExportedToastTitle", "Export completed"),
          },
          error: {
            title: t("launcherExportFailedTitle", "Failed to export launcher items"),
          },
        },
        () => invoke("launcher_export", { outputPath }),
      );
    } catch (err) {
      console.error(err);
    }
  };

  const handleImport = async () => {
    if (!isTauri) return;
    try {
      const importPath = await open({
        multiple: false,
        filters: [{ name: "JSON", extensions: ["json"] }],
      });
      if (!importPath || Array.isArray(importPath)) return;
      const resp = await invoke<ApiResp<{ count: number; total: number }>>(
        "launcher_import",
        {
          importPath,
          mode: "merge",
        },
      );
      await safeRecordMessage({
        source: "launcher",
        category: "import",
        severity: "success",
        title: t("launcherImportMessageTitle", "Launcher items imported"),
        summary: t("launcherImportSuccess", { count: resp.data?.count ?? 0 }),
        dedupe_key: "launcher:import",
        target: { tab: "launcher" },
        metadata: { import_path: importPath, imported: resp.data?.count ?? 0 },
      });
      await refreshLauncherItems();
      emit("refresh-counts").catch(() => {});
      pushToast({
        title: t("launcherImportSuccess", { count: resp.data?.count ?? 0 }),
        kind: "success",
      });
    } catch (err) {
      console.error(err);
      await safeRecordMessage({
        source: "launcher",
        category: "import",
        severity: "error",
        title: t("launcherImportFailedTitle", "Failed to import launcher items"),
        summary: t("launcherImportFailed", { error: formatInvokeError(err) }),
        detail: errorToMessage(err),
        dedupe_key: "launcher:import",
        target: { tab: "launcher" },
      });
      pushToast({
        title: t("launcherImportFailed", { error: formatInvokeError(err) }),
        kind: "error",
      });
    }
  };

  const handleSelectApplication = async () => {
    if (!isTauri) return;
    try {
      const selected = await open({
        multiple: false,
        directory: false,
        defaultPath: "/Applications",
        filters: [{ name: "Applications", extensions: ["app"] }],
      });
      if (selected && typeof selected === "string") {
        const fileName = selected.split("/").pop() || selected;
        const appName = fileName.endsWith(".app")
          ? fileName.slice(0, -4)
          : fileName;
        if (appName) {
          setTargetInput(appName);
          setNameInput(appName);
        }
      }
    } catch (err) {
      console.error(err);
    }
  };

  const handleSelectFolder = async () => {
    if (!isTauri) return;
    try {
      const selected = await open({
        multiple: false,
        directory: true,
      });
      if (selected && typeof selected === "string") {
        setTargetInput(selected);
      }
    } catch (err) {
      console.error(err);
    }
  };

  return (
    <div className="flex flex-col h-full space-y-5">
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-xl font-bold tracking-tight">
            {t("launcher", "Launcher")}
          </h2>
          <p className="text-sm text-muted-foreground mt-1">
            {t(
              "launcherDesc",
              "Quickly launch favorite apps, local directories, and automated workflows",
            )}
          </p>
        </div>
        <div className="flex items-center gap-2">
          <button
            onClick={handleImport}
            className="bg-secondary text-secondary-foreground hover:bg-secondary/90 px-3 py-2 rounded-md flex items-center gap-2 text-sm font-medium transition-colors"
          >
            <Upload className="w-4 h-4" />
            {t("import", "Import")}
          </button>
          <button
            onClick={handleExport}
            className="bg-secondary text-secondary-foreground hover:bg-secondary/90 px-3 py-2 rounded-md flex items-center gap-2 text-sm font-medium transition-colors"
          >
            <Download className="w-4 h-4" />
            {t("export", "Export")}
          </button>
          <button
            onClick={startCreate}
            className="bg-primary text-primary-foreground hover:bg-primary/90 px-4 py-2 rounded-md flex items-center gap-2 text-sm font-medium transition-colors shadow-sm"
          >
            <Plus className="w-4 h-4" />
            {t("addShortcut", "Add Shortcut")}
          </button>
        </div>
      </div>

      <div className="relative">
        <Search className="w-4 h-4 absolute left-3 top-3 text-muted-foreground" />
        <input
          type="text"
          placeholder={t("searchLauncher", "Search launcher items...")}
          value={searchTerm}
          onChange={(e) => setSearchTerm(e.target.value)}
          className="w-full flex h-10 rounded-md border border-input bg-background pl-9 pr-3 py-2 text-sm"
        />
      </div>

      {isEditing && (
        <div className="bg-card border rounded-xl p-5 shadow-sm space-y-4">
          <h3 className="font-semibold flex items-center gap-2">
            <Rocket className="w-4 h-4 text-primary" />
            {editingId
              ? t("editShortcut", "Edit Shortcut")
              : t("newShortcut", "New Shortcut")}
          </h3>
          <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
            <div className="space-y-2">
              <label className="text-xs font-medium text-muted-foreground uppercase tracking-wider">
                {t("name")}
              </label>
              <input
                type="text"
                placeholder={t("appNamePlaceholder", "e.g. My App")}
                value={nameInput}
                onChange={(e) => setNameInput(e.target.value)}
                className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
              />
            </div>
            <div className="space-y-2">
              <label className="text-xs font-medium text-muted-foreground uppercase tracking-wider">
                {t("type", "Type")}
              </label>
              <select
                value={typeInput}
                onChange={(e) =>
                  setTypeInput(e.target.value as LauncherItem["type"])
                }
                className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
              >
                <option value="app">
                  {t("macApp", "Mac Application (open -a)")}
                </option>
                <option value="script">
                  {t("shellCommand", "Shell Command")}
                </option>
                <option value="url">{t("websiteUrl", "Website URL")}</option>
                <option value="folder">
                  {t("localFolder", "Local Folder")}
                </option>
                <option value="internal">
                  {t("internalAction", "Internal Action")}
                </option>
              </select>
            </div>

            {typeInput === "internal" ? (
              <div className="space-y-2 md:col-span-2">
                <label className="text-xs font-medium text-muted-foreground uppercase tracking-wider">
                  {t("targetModule", "Target Module")}
                </label>
                <select
                  value={targetInput}
                  onChange={(e) => setTargetInput(e.target.value)}
                  className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
                >
                  <option value="">
                    {t("selectModule", "Select module...")}
                  </option>
                  {INTERNAL_TARGETS.map((target) => (
                    <option key={target.id} value={target.id}>
                      {target.id === "ai-assistants"
                        ? smartWorkspaceLabel
                        : t(target.labelKey, target.fallback)}
                    </option>
                  ))}
                </select>
              </div>
            ) : typeInput === "app" ? (
              <div className="space-y-2 md:col-span-2">
                <label className="text-xs font-medium text-muted-foreground uppercase tracking-wider">
                  {t("launchTarget", "Command / Path / URL")}
                </label>
                <div className="flex gap-2">
                  <input
                    type="text"
                    placeholder={t(
                      "selectAppFromApplications",
                      "Choose app from Applications",
                    )}
                    value={targetInput}
                    readOnly
                    className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
                  />
                  <button
                    onClick={handleSelectApplication}
                    className="bg-secondary text-secondary-foreground hover:bg-secondary/80 px-4 py-2 rounded-md flex items-center gap-2 text-sm font-medium transition-colors shrink-0"
                  >
                    <FolderOpen className="w-4 h-4" />
                    {t("browse", "Browse")}
                  </button>
                </div>
              </div>
            ) : typeInput === "folder" ? (
              <div className="space-y-2 md:col-span-2">
                <label className="text-xs font-medium text-muted-foreground uppercase tracking-wider">
                  {t("launchTarget", "Command / Path / URL")}
                </label>
                <div className="flex gap-2">
                  <input
                    type="text"
                    placeholder={t("selectFolderPath", "Choose folder")}
                    value={targetInput}
                    readOnly
                    className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
                  />
                  <button
                    onClick={handleSelectFolder}
                    className="bg-secondary text-secondary-foreground hover:bg-secondary/80 px-4 py-2 rounded-md flex items-center gap-2 text-sm font-medium transition-colors shrink-0"
                  >
                    <FolderOpen className="w-4 h-4" />
                    {t("browse", "Browse")}
                  </button>
                </div>
              </div>
            ) : (
              <div className="space-y-2 md:col-span-2">
                <label className="text-xs font-medium text-muted-foreground uppercase tracking-wider">
                  {t("launchTarget", "Command / Path / URL")}
                </label>
                <input
                  type="text"
                  placeholder={t("pathOrUrlPlaceholder", "Path or URL...")}
                  value={targetInput}
                  onChange={(e) => setTargetInput(e.target.value)}
                  className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm font-mono"
                />
              </div>
            )}

            <div className="md:col-span-2">
              <label className="inline-flex items-center gap-2 text-sm text-muted-foreground">
                <input
                  type="checkbox"
                  checked={pinnedInput}
                  onChange={(e) => setPinnedInput(e.target.checked)}
                  className="w-4 h-4"
                />
                {t("pinShortcut", "Pin this shortcut")}
              </label>
            </div>
          </div>

          <div className="flex justify-end gap-3 pt-2">
            <button
              onClick={resetEditor}
              className="px-4 py-2 rounded-md text-sm font-medium hover:bg-muted transition-colors"
            >
              {t("cancel")}
            </button>
            <button
              onClick={handleSave}
              disabled={!nameInput.trim() || !targetInput.trim()}
              className="bg-primary text-primary-foreground hover:bg-primary/90 px-4 py-2 rounded-md text-sm font-medium transition-colors disabled:opacity-50"
            >
              {t("save")}
            </button>
          </div>
        </div>
      )}

      {loading ? (
        <div className="text-sm text-muted-foreground">
          {t("loading", "Loading...")}
        </div>
      ) : groupedItems.length === 0 && quickInternalTools.length === 0 ? (
        <div className="text-sm text-muted-foreground">
          {t("noResultsFound", "No results found.")}
        </div>
      ) : (
        <div className="space-y-6">
          {quickInternalTools.length > 0 ? (
            <section className="space-y-3">
              <div>
                <h3 className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">
                  {t("launcherInternalTools", "Internal Tools")}
                </h3>
                <p className="mt-1 text-sm text-muted-foreground">
                  {t(
                    "launcherInternalToolsDesc",
                    "Keep internal utilities close at hand without expanding the sidebar.",
                  )}
                </p>
              </div>

              <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-4">
                {quickInternalTools.map((item) => {
                  const Icon = item.icon;
                  return (
                    <button
                      key={item.id}
                      type="button"
                      onClick={() => openInternalTarget(item.target)}
                      className="group flex min-h-36 flex-col justify-between rounded-xl border bg-card p-4 text-left shadow-sm transition-all hover:border-primary/50 hover:shadow-md"
                    >
                      <div className="flex items-start justify-between gap-3">
                        <div className="rounded-lg bg-emerald-500/10 p-2 text-emerald-500">
                          <Icon className="h-6 w-6" />
                        </div>
                        <div className="flex flex-col items-end gap-2">
                          {item.statusBadge}
                          <span className="rounded-full border bg-muted px-2 py-0.5 text-[10px] uppercase tracking-[0.16em] text-muted-foreground">
                            {t("launcherPinnedEntry", "Pinned")}
                          </span>
                        </div>
                      </div>
                      <div className="space-y-1">
                        <div className="font-semibold">{item.name}</div>
                        <p className="text-sm leading-6 text-muted-foreground">
                          {item.description}
                        </p>
                      </div>
                    </button>
                  );
                })}
              </div>
            </section>
          ) : null}

          {groupedItems.map((group) => (
            <section key={group.type} className="space-y-3">
              <h3 className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">
                {typeLabelMap[group.type]}
              </h3>
              <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-4">
                {group.items.map((item) => {
                  const Icon = launcherIcon(item.type);
                  const pinnedIndex = pinnedOrderIds.findIndex(
                    (id) => id === item.id,
                  );
                  const isPinned = item.pinned;
                  const lastUsed = item.last_launched_at
                    ? formatRelativeTime(item.last_launched_at)
                    : t("neverLaunched", "Never launched");
                  const appIconDataUrl =
                    item.type === "app"
                      ? appIconCache[appIconCacheKey(item.target)]
                      : null;

                  return (
                    <div
                      key={item.id}
                      onClick={() => handleLaunch(item)}
                      className="group flex flex-col justify-between p-4 rounded-xl border bg-card text-card-foreground shadow-sm hover:shadow-md transition-all hover:border-primary/50 cursor-pointer min-h-40"
                    >
                      <div className="flex justify-between items-start gap-2">
                        <div
                          className={`p-2 rounded-lg ${item.type === "app" ? "bg-blue-500/10 text-blue-500" : item.type === "internal" ? "bg-emerald-500/10 text-emerald-500" : "bg-primary/10 text-primary"}`}
                        >
                          {item.type === "app" && appIconDataUrl ? (
                            <img
                              src={appIconDataUrl}
                              alt={`${item.name} icon`}
                              className="w-7 h-7 rounded-sm object-contain"
                            />
                          ) : (
                            <Icon className="w-7 h-7" />
                          )}
                        </div>

                        <div className="flex items-center gap-1 opacity-0 group-hover:opacity-100 transition-all">
                          <button
                            onClick={(e) => handleTogglePin(item, e)}
                            className="text-muted-foreground hover:text-foreground p-1 rounded-md"
                            title={
                              isPinned ? t("unpin", "Unpin") : t("pin", "Pin")
                            }
                          >
                            {isPinned ? (
                              <PinOff className="w-4 h-4" />
                            ) : (
                              <Pin className="w-4 h-4" />
                            )}
                          </button>
                          {isPinned && (
                            <>
                              <button
                                onClick={(e) =>
                                  handleMovePinned(item.id, "up", e)
                                }
                                disabled={pinnedIndex <= 0}
                                className="text-muted-foreground hover:text-foreground p-1 rounded-md disabled:opacity-40"
                                title={t("moveUp", "Move Up")}
                              >
                                <ArrowUp className="w-4 h-4" />
                              </button>
                              <button
                                onClick={(e) =>
                                  handleMovePinned(item.id, "down", e)
                                }
                                disabled={
                                  pinnedIndex < 0 ||
                                  pinnedIndex >= pinnedOrderIds.length - 1
                                }
                                className="text-muted-foreground hover:text-foreground p-1 rounded-md disabled:opacity-40"
                                title={t("moveDown", "Move Down")}
                              >
                                <ArrowDown className="w-4 h-4" />
                              </button>
                            </>
                          )}
                          <button
                            onClick={(e) => {
                              e.stopPropagation();
                              startEdit(item);
                            }}
                            className="text-muted-foreground hover:text-foreground p-1 rounded-md"
                            title={t("edit", "Edit")}
                          >
                            <Edit className="w-4 h-4" />
                          </button>
                          <button
                            onClick={(e) => handleDelete(item, e)}
                            className="text-muted-foreground hover:text-destructive p-1 rounded-md"
                            title={t("delete", "Delete")}
                          >
                            <Trash2 className="w-4 h-4" />
                          </button>
                        </div>
                      </div>

                      <div className="mt-3 space-y-1">
                        <div className="flex items-center gap-2">
                          <h3 className="font-semibold truncate">
                            {item.name}
                          </h3>
                          {isPinned && (
                            <span className="text-[10px] px-1.5 py-0.5 rounded-full bg-primary/10 text-primary">
                              {t("pinned", "Pinned")}
                            </span>
                          )}
                        </div>
                        <p className="text-xs text-muted-foreground truncate font-mono opacity-80">
                          {item.target}
                        </p>
                        <p className="text-xs text-muted-foreground">
                          {t("launchCount", "Launches")}: {item.launch_count} ·{" "}
                          {t("lastUsed", "Last used")}: {lastUsed}
                        </p>
                      </div>
                    </div>
                  );
                })}
              </div>
            </section>
          ))}
        </div>
      )}

      {pendingScriptItem && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-background/80 backdrop-blur-sm p-4">
          <div className="bg-card border rounded-xl shadow-lg w-full max-w-sm overflow-hidden animate-in fade-in zoom-in-95 duration-200">
            <div className="p-5 space-y-3">
              <div className="flex items-center gap-3 text-amber-600 dark:text-amber-500">
                <div className="bg-amber-500/10 p-2 rounded-full">
                  <ShieldAlert className="w-5 h-5" />
                </div>
                <h3 className="font-semibold">
                  {t("launcherScriptConfirmTitle", "Run untrusted command?")}
                </h3>
              </div>
              <p className="text-sm text-muted-foreground break-all">
                {pendingScriptItem.target}
              </p>
              <label className="inline-flex items-center gap-2 text-sm">
                <input
                  type="checkbox"
                  checked={trustOnConfirm}
                  onChange={(e) => setTrustOnConfirm(e.target.checked)}
                  className="w-4 h-4"
                />
                {t(
                  "launcherTrustThisItem",
                  "Trust this launcher item for future runs",
                )}
              </label>
            </div>
            <div className="p-4 bg-muted/30 border-t flex justify-end gap-3">
              <button
                onClick={cancelScriptLaunch}
                className="px-4 py-2 rounded-md text-sm font-medium hover:bg-muted transition-colors"
              >
                {t("cancel")}
              </button>
              <button
                onClick={confirmScriptLaunch}
                className="px-4 py-2 rounded-md text-sm font-medium bg-primary text-primary-foreground hover:bg-primary/90 transition-colors"
              >
                {t("launch", "Launch")}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
