import { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import { emit } from "@tauri-apps/api/event";
import { open, save } from "@tauri-apps/plugin-dialog";
import {
  deriveSshTunnelLauncherSummary,
  type LauncherSshTunnelSummary,
} from "@/lib/sshTunnelSummary";
import { type ProtocolRouterStatus } from "@/lib/protocolRouter";
import { useRuntimeStatus } from "@/lib/runtimeStatus";
import { useConfirmDialog } from "@/components/ConfirmDialogProvider";
import { useToast } from "@/components/ToastProvider";
import type { SshTunnelsSnapshot } from "@/components/sshTunnels/types";
import { notifyMessage, safeRecordMessage } from "@/lib/messages";
import { runUserAction } from "@/lib/userActions";
import {
  LAUNCHER_TOOL_VISIBILITY_UPDATED_EVENT,
  readLauncherToolVisibility,
} from "@/lib/launcherToolVisibility";
import {
  LAUNCHER_INTERNAL_TOOLS_ORDER_KEY,
  applySavedOrder,
  readSavedOrder,
  writeSavedOrder,
} from "@/lib/launcherToolOrder";
import { useCardDragReorder } from "@/lib/useCardDragReorder";
import { ToolStatusBadge } from "@/components/toolbox/ToolStatusBadge";
import { listToolboxTools, resolveToolboxText } from "@/toolbox/registry";
import {
  DEFAULT_LAUNCHER_ITEMS,
  LAUNCHER_TYPE_ORDER,
  SEEDED_MARKER,
} from "../constants";
import { formatInvokeError, sortLauncherItems } from "../helpers";
import type {
  ApiResp,
  LauncherItem,
  LauncherItemInput,
  LauncherQuickToolEntry,
  LauncherWindowBindings,
} from "../types";

export function useLauncherController(isVisible: boolean) {
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

  const isTauri = "__TAURI_INTERNALS__" in window;

  // Status cards read the shared runtime-status store; the App-level event
  // bridge publishes snapshots, so the launcher never pulls per event.
  const tunnelsRuntime = useRuntimeStatus<SshTunnelsSnapshot>("ssh-tunnels", {
    enabled: isVisible && isTauri,
  });
  const routerRuntime = useRuntimeStatus<ProtocolRouterStatus>("router", {
    enabled: isVisible && isTauri,
  });
  const sshTunnelSummary = useMemo(
    () =>
      tunnelsRuntime.data
        ? deriveSshTunnelLauncherSummary(tunnelsRuntime.data)
        : null,
    [tunnelsRuntime.data],
  );
  const protocolRouterStatusState = routerRuntime.data;

  const [toolVisibility, setToolVisibility] = useState(
    readLauncherToolVisibility,
  );
  const [internalToolsOrder, setInternalToolsOrder] = useState<string[]>(
    () => readSavedOrder(LAUNCHER_INTERNAL_TOOLS_ORDER_KEY),
  );

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

  const renderSshTunnelStatus = (summary?: LauncherSshTunnelSummary | null) => {
    if (!summary) return null;

    if (summary.state === "failed") {
      const label = t("launcherSshTunnelFailedAria", {
        count: summary.autoConnectFailedCount,
        defaultValue: `${summary.autoConnectFailedCount} auto-connect SSH tunnels failed`,
      });
      return (
        <span aria-label={label} title={label}>
          <ToolStatusBadge
            tone="error"
            label={String(summary.autoConnectFailedCount)}
          />
        </span>
      );
    }

    if (summary.state === "connecting") {
      const label = t(
        "launcherSshTunnelConnectingAria",
        "SSH tunnels are connecting automatically.",
      );
      return (
        <span aria-label={label} title={label}>
          <ToolStatusBadge
            tone="warning"
            label={t("launcherSshTunnelConnecting", "Connecting...")}
          />
        </span>
      );
    }

    const label = t("launcherSshTunnelConnectedAria", {
      count: summary.connectedCount,
      defaultValue: `${summary.connectedCount} SSH tunnels connected`,
    });
    return (
      <span aria-label={label} title={label}>
        <ToolStatusBadge
          tone="success"
          label={String(summary.connectedCount)}
        />
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
        <span aria-label={label} title={label}>
          <ToolStatusBadge
            tone="success"
            label={t("launcherProtocolRouterRunning", "Running")}
          />
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
      <span aria-label={label} title={label}>
        <ToolStatusBadge
          tone="neutral"
          label={
            status.enabled
              ? t("launcherProtocolRouterStopped", "Stopped")
              : t("launcherProtocolRouterDisabled", "Disabled")
          }
        />
      </span>
    );
  };

  const quickInternalTools = useMemo(() => {
    const entries: LauncherQuickToolEntry[] = [];

    for (const tool of listToolboxTools("launcher-quick")) {
      const bilingualSearchText = ["en", "zh"]
        .flatMap((language) => {
          const translate = i18n.getFixedT(language);
          return [
            resolveToolboxText(
              tool.labelText,
              tool.labelKey,
              translate,
              language,
            ),
            resolveToolboxText(
              tool.descriptionText,
              tool.descriptionKey,
              translate,
              language,
            ),
          ];
        })
        .join(" ");

      entries.push({
        id: `quick-${tool.id}`,
        name: resolveToolboxText(tool.labelText, tool.labelKey, t),
        description: resolveToolboxText(
          tool.descriptionText,
          tool.descriptionKey,
          t,
        ),
        target: tool.id,
        icon: tool.icon,
        iconClassName: tool.iconClassName,
        statusBadge:
          tool.id === "ssh-tunnels"
            ? renderSshTunnelStatus(sshTunnelSummary)
            : tool.id === "protocol-router"
              ? renderProtocolRouterStatus(protocolRouterStatusState)
              : null,
        visible: toolVisibility[tool.id],
        searchText: bilingualSearchText,
      });

      for (const alias of tool.aliases) {
        if (alias.target === tool.id) continue;
        entries.push({
          id: `quick-${tool.id}-${alias.target}`,
          name: `JT/T ${alias.target}`,
          description: `Open the ${alias.target} parser tab.`,
          target: alias.target,
          icon: tool.icon,
          iconClassName: tool.iconClassName,
          statusBadge: null,
          visible: toolVisibility[tool.id],
          aliasOnly: true,
        });
      }
    }

    const term = searchTerm.trim().toLowerCase();
    const visibleItems = entries.filter(
      (item) => item.visible && (term !== "" || !item.aliasOnly),
    );
    const orderedItems = applySavedOrder(visibleItems, internalToolsOrder);

    if (!term) return orderedItems;
    return orderedItems.filter((item) => {
      const searchText = item.searchText ?? "";
      return `${item.name} ${item.description} ${item.target} ${searchText}`
        .toLowerCase()
        .includes(term);
    });
  }, [i18n, internalToolsOrder, protocolRouterStatusState, searchTerm, sshTunnelSummary, t, toolVisibility]);

  const drag = useCardDragReorder({
    ids: quickInternalTools.map((item) => item.id),
    onReorder: (next) => {
      setInternalToolsOrder(next);
      writeSavedOrder(LAUNCHER_INTERNAL_TOOLS_ORDER_KEY, next);
    },
  });

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
        const seeded = await seedDefaultLauncherIfNeeded();
        if (seeded) {
          loaded = await listLauncherItems();
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
      void notifyMessage.error({
        source: "launcher",
        category: "execute",
        title: t("launcherLaunchFailedMessageTitle", "Launcher failed to start"),
        summary: `${item.name}: ${err instanceof Error ? err.message : "Launch failed"}`,
        error: err,
        dedupeKey: `launcher:execute:error:${item.id}`,
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
      await notifyMessage.success({
        source: "launcher",
        category: "execute",
        title: t("launcherLaunchSuccessMessageTitle", "Launcher action started"),
        summary: t("launcherLaunchSuccessSummary", {
          name: item.name,
          defaultValue: "{{name}} started successfully.",
        }),
        dedupeKey: `launcher:execute:success:${item.id}`,
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
      void notifyMessage.error({
        source: "launcher",
        category: "execute",
        title: t("launcherLaunchFailedMessageTitle", "Launcher failed to start"),
        summary: `${item.name}: ${err instanceof Error ? err.message : "Launch failed"}`,
        error: err,
        dedupeKey: `launcher:execute:error:${item.id}`,
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
      await notifyMessage.success({
        source: "launcher",
        category: "import",
        title: t("launcherImportMessageTitle", "Launcher items imported"),
        summary: t("launcherImportSuccess", { count: resp.data?.count ?? 0 }),
        dedupeKey: "launcher:import",
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
      await notifyMessage.error({
        source: "launcher",
        category: "import",
        title: t("launcherImportFailedTitle", "Failed to import launcher items"),
        summary: t("launcherImportFailed", { error: formatInvokeError(err) }),
        error: err,
        dedupeKey: "launcher:import",
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

  return {
    t,
    items,
    loading,
    searchTerm,
    setSearchTerm,
    isEditing,
    editingId,
    nameInput,
    setNameInput,
    typeInput,
    setTypeInput,
    targetInput,
    setTargetInput,
    pinnedInput,
    setPinnedInput,
    pendingScriptItem,
    trustOnConfirm,
    setTrustOnConfirm,
    groupedItems,
    quickInternalTools,
    drag,
    typeLabelMap,
    pinnedOrderIds,
    appIconCache,
    appIconCacheKey,
    formatRelativeTime,
    openInternalTarget,
    handleImport,
    handleExport,
    startCreate,
    resetEditor,
    handleSave,
    handleTogglePin,
    handleMovePinned,
    startEdit,
    handleDelete,
    handleLaunch,
    confirmScriptLaunch,
    cancelScriptLaunch,
    handleSelectApplication,
    handleSelectFolder,
  };
}
