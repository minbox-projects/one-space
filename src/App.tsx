import { useState, useEffect, useMemo, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useTranslation } from "react-i18next";
import { useTheme } from "./components/ThemeProvider";
import { useToast } from "./components/ToastProvider";
import {
  Bell,
  Rocket,
  Terminal,
  FolderOpen,
  Sparkles,
  Newspaper,
  Search,
  Mail as MailIcon,
  Settings,
  Moon,
  Sun,
  Monitor,
  Cpu,
  BookOpen,
  Info,
  Github,
  Fish,
  Bot,
  Loader2,
  Waypoints,
  CheckCircle2,
  AlertCircle,
  ArrowUpCircle,
  Check,
  Code2,
  Copy,
  NotebookPen,
  X,
  Route,
  BarChart3,
} from "lucide-react";
import { AiSessions } from "./components/AiSessions";
import { Workspaces } from "./components/Workspaces";
import { AiEnvironments } from "./components/AiEnvironments";
import { AiUsageStats } from "./components/AiUsageStats";
import { AiFlow } from "./components/AiFlow";
import { Skills } from "./components/Skills";
import { Subagents } from "./components/Subagents";
import { MCPServers } from "./components/MCPServers";
import { Mail } from "./components/Mail";
import { AiNews } from "./components/AiNews";
import { OmniSearch } from "./components/OmniSearch";
import { Launcher } from "./components/Launcher";
import { MoreToolsHub } from "./components/MoreToolsHub";
import { Notes } from "./components/Notes";
import { SettingsView } from "./components/SettingsView";
import { Snippets } from "./components/Snippets";
import { AboutModal } from "./components/AboutModal";
import { QuickAiSessionBar } from "./components/QuickAiSessionBar";
import { QuickAssistantWindow } from "./components/QuickAssistantWindow";
import { SmartWorkspaceHub } from "./components/SmartWorkspaceHub";
import { Documentation } from "./components/Documentation";
import { OnboardingWizard } from "./components/OnboardingWizard";
import { FishPond } from "./components/FishPond";
import { protocolRouterStatus, type ProtocolRouterStatus } from "./lib/protocolRouter";
import { UpdateUpgradeModal } from "./components/UpdateUpgradeModal";
import { AppErrorBoundary } from "./components/AppErrorBoundary";
import { MessageCenter } from "./components/MessageCenter";
import type {
  CapabilityTargetTab,
  WorkspaceCapabilityContext,
} from "./components/workspaceCapabilityContext";
import { getUpdaterState, useUpdater } from "./lib/updater";
import {
  NETWORK_CIRCUIT_EVENT,
  NETWORK_CIRCUIT_MESSAGE,
} from "./lib/networkCircuitBreaker";
import {
  isMoreToolsTab,
  isSmartWorkspaceTab,
  normalizeLegacyTabTarget,
  resolveNavigationTarget,
  type MoreToolsSection,
  type SmartWorkspaceSection,
} from "./lib/navigation";
import { deriveSshTunnelHeaderSummary } from "./lib/sshTunnelSummary";
import {
  errorToMessage,
  getUnreadMessageCount,
  MESSAGES_UPDATED_EVENT,
  recordMessage,
  type MessageTarget,
} from "./lib/messages";
import { openExternalUrl } from "./lib/externalActions";
import { notifySystemEvent } from "./lib/userActions";
import {
  buildSshAutoConnectFailedEvent,
  buildSshUnexpectedDisconnectEvent,
  buildSshWindowReconnectDoneEvent,
  buildUpdaterSystemEvent,
} from "./lib/actionDescriptors/appSystemEvents";

import { getUnreadEmailCount } from "./lib/gmail";
import logoWhite from "./assets/onespace_logo_white.png";
import logoBlack from "./assets/onespace_logo_black.png";

type ApiResp<T> = {
  ok: boolean;
  data: T;
  meta: { schema_version: number; revision: number };
};
type TrayActionPayload = { action?: string; target?: string };
type AppStorageConfig = {
  language?: string;
  storage_type?: "local" | "git" | "icloud";
  update_ignored_version?: string | null;
  auto_update_enabled?: boolean;
  update_check_interval_minutes?: number;
  message_retention_days?: number;
  skills_sync_enabled?: boolean;
  skills_sync_interval_minutes?: number;
  skills_auto_update_enabled?: boolean;
  ai_news_enabled?: boolean;
  ai_news_sync_interval_minutes?: number;
  ai_news_rss_sources?: Array<{
    id: string;
    name: string;
    url: string;
    enabled: boolean;
  }>;
  subagents_sync_enabled?: boolean;
  subagents_sync_interval_minutes?: number;
};
type RepoAutoUpdateResult = {
  updated_repo_keys: string[];
  updated_skill_names: string[];
  synced_targets: Array<{
    model: string;
    scope: string;
    project_root?: string | null;
    dir_name: string;
  }>;
  updated_repo_count: number;
  synced_target_count: number;
  applied_at: number;
};
const SKILLS_AUTO_UPDATED_EVENT = "onespace:skills-auto-updated";

type DashboardCounts = {
  launcher: number;
  workspaces: number;
  sessions: number;
  ssh: number;
  snippets: number;
  bookmarks: number;
  notes: number;
  ai_news: number;
  environments: number;
  skills: number;
  subagents: number;
  mcp_servers: number;
  storage_type?: "local" | "git" | "icloud";
};

const TRAY_NAV_TABS = new Set([
  "launcher",
  "workspaces",
  "ai-sessions",
  "ai-assistants",
  "ai-assistants-library",
  "ai-automations",
  "ai-model-center",
  "ai-environments",
  "ai-usage",
  "ai-flow",
  "ai-news",
  "more-tools",
  "skills",
  "subagents",
  "mcp-servers",
  "ssh",
  "ssh-tunnels",
  "protocol-router",
  "snippets",
  "bookmarks",
  "notes",
  "cloud",
  "mail",
  "documentation",
]);

const MCPIcon = ({ className }: { className?: string }) => (
  <svg
    viewBox="0 0 180 180"
    fill="none"
    xmlns="http://www.w3.org/2000/svg"
    className={className}
    aria-hidden="true"
  >
    <path
      d="M23.5996 85.2532L86.2021 22.6507C94.8457 14.0071 108.86 14.0071 117.503 22.6507C126.147 31.2942 126.147 45.3083 117.503 53.9519L70.2254 101.23"
      stroke="currentColor"
      strokeWidth="11.0667"
      strokeLinecap="round"
    />
    <path
      d="M70.8789 100.578L117.504 53.952C126.148 45.3083 140.163 45.3083 148.806 53.952L149.132 54.278C157.776 62.9216 157.776 76.9357 149.132 85.5792L92.5139 142.198C89.6327 145.079 89.6327 149.75 92.5139 152.631L104.14 164.257"
      stroke="currentColor"
      strokeWidth="11.0667"
      strokeLinecap="round"
    />
    <path
      d="M101.853 38.3013L55.553 84.6011C46.9094 93.2447 46.9094 107.258 55.553 115.902C64.1966 124.546 78.2106 124.546 86.8543 115.902L133.154 69.6025"
      stroke="currentColor"
      strokeWidth="11.0667"
      strokeLinecap="round"
    />
  </svg>
);

type AppWindowBindings = typeof window & {
  setActiveTab?: (tab: string) => void;
  setSettingsOpen?: (open: boolean) => void;
  setSettingsTab?: (tab: string) => void;
};

function App() {
  const { t, i18n } = useTranslation();
  const { theme, setTheme } = useTheme();
  const { pushToast, dismissToast } = useToast();

  // URL View Routing
  const queryParams = new URLSearchParams(window.location.search);
  const view = queryParams.get("view");
  const isQuickAiView = view === "quick-ai";
  const isQuickAssistantView = view === "quick-assistant";
  const isSelectionAssistantView = view === "selection-assistant";

  const [activeTab, setActiveTab] = useState("launcher");
  const [previousTab, setPreviousTab] = useState("launcher");
  const [fishPondPreviousTab, setFishPondPreviousTab] = useState("launcher");
  const [omniOpen, setOmniOpen] = useState(false);
  const [settingsInitialTab, setSettingsInitialTab] = useState("storage");
  const [aboutOpen, setAboutOpen] = useState(false);
  const [messageCenterOpen, setMessageCenterOpen] = useState(false);
  const [messageUnreadCount, setMessageUnreadCount] = useState(0);
  const [smartWorkspaceSection, setSmartWorkspaceSection] =
    useState<SmartWorkspaceSection>("conversations");
  const [moreToolsSection, setMoreToolsSection] =
    useState<MoreToolsSection>("bookmarks");
  const [storageType, setStorageType] = useState<"local" | "git" | "icloud">(
    "local",
  );
  const [onboardingStatus, setOnboardingStatus] = useState<
    "checking" | "required" | "done"
  >("checking");
  const [mountedTabs, setMountedTabs] = useState<Set<string>>(
    () => new Set(["launcher"]),
  );

  // Git Sync Status
  const [syncStatus, setSyncStatus] = useState<
    "idle" | "pulling" | "pushing" | "success" | "error"
  >("idle");
  const [syncError, setSyncError] = useState<string | null>(null);
  const [networkCircuitOpen, setNetworkCircuitOpen] = useState(false);
  const [sshTunnelSummary, setSshTunnelSummary] = useState<{
    connectedCount: number;
    hasErrors: boolean;
    hasConnecting: boolean;
    errorTunnelNames: string[];
  } | null>(null);
  const [protocolRouterHeaderStatus, setProtocolRouterHeaderStatus] =
    useState<ProtocolRouterStatus | null>(null);
  const sshTunnelSummaryRef = useRef<{
    connectedCount: number;
    hasErrors: boolean;
    hasConnecting: boolean;
    errorTunnelNames: string[];
  } | null>(null);
  const sshReconnectLoadingToastRef = useRef<string | null>(null);
  const [skillsAutoUpdateNotice, setSkillsAutoUpdateNotice] = useState<
    string | null
  >(null);
  const [updateDialogOpen, setUpdateDialogOpen] = useState(false);
  const [autoUpdateNoticeOpen, setAutoUpdateNoticeOpen] = useState(false);
  const [ignoredUpdateVersion, setIgnoredUpdateVersion] = useState<
    string | null
  >(null);
  const [runtimeError, setRuntimeError] = useState<string | null>(null);
  const [runtimeErrorCopied, setRuntimeErrorCopied] = useState(false);
  const [workspaceCapabilityNavigation, setWorkspaceCapabilityNavigation] =
    useState<{
      targetTab: CapabilityTargetTab;
      context: WorkspaceCapabilityContext;
    } | null>(null);
  const ignoredUpdateVersionRef = useRef<string | null>(null);
  const activeTabRef = useRef(activeTab);
  const {
    status: updaterStatus,
    manifest: updaterManifest,
    installable: updaterInstallable,
    downloadProgress: updaterDownloadProgress,
    checkForUpdates,
    downloadUpdateIfAvailable,
    installDownloadedUpdate,
    installUpdate,
    error: updaterError,
  } = useUpdater();
  const updateMessageKeyRef = useRef<string | null>(null);

  // Global counts for sidebar
  const [counts, setCounts] = useState({
    launcher: 0,
    workspaces: 0,
    sessions: 0,
    ssh: 0,
    snippets: 0,
    bookmarks: 0,
    notes: 0,
    aiNews: 0,
    mail: 0,
    environments: 0,
    skills: 0,
    subagents: 0,
    mcpServers: 0,
  });
  const loadCountsInFlightRef = useRef<Promise<void> | null>(null);
  const countsRefreshTimerRef = useRef<number | null>(null);

  const isTauri = "__TAURI_INTERNALS__" in window;
  const smartWorkspaceLabel =
    i18n.language === "zh" ? "AI 工作台" : "AI Workspace";
  const moreToolsLabel =
    i18n.language === "zh" ? "更多工具" : "More Tools";

  const navigateToTab = (target: string) => {
    const resolved = resolveNavigationTarget(target);

    if (resolved.smartWorkspaceSection) {
      setSmartWorkspaceSection(resolved.smartWorkspaceSection);
    }
    if (resolved.moreToolsSection) {
      setMoreToolsSection(resolved.moreToolsSection);
    }

    if (resolved.tab === "settings") {
      const currentTab = activeTabRef.current;
      if (currentTab !== "settings") {
        setPreviousTab(currentTab);
      }
      setSettingsInitialTab("storage");
      setActiveTab("settings");
      return;
    }

    setActiveTab(resolved.tab);
  };

  const navigateToMessageTarget = (target: MessageTarget) => {
    setMessageCenterOpen(false);
    if (target.tab === "settings") {
      const currentTab = activeTabRef.current;
      if (currentTab !== "settings") {
        setPreviousTab(currentTab);
      }
      setSettingsInitialTab(target.section || "storage");
      setActiveTab("settings");
      return;
    }
    navigateToTab(target.tab);
  };

  useEffect(() => {
    activeTabRef.current = activeTab;
  }, [activeTab]);

  useEffect(() => {
    const onNetworkCircuitOpen = () => {
      setNetworkCircuitOpen(true);
      void notifySystemEvent(
        { pushToast, recordMessage },
        {
          source: "system",
          category: "network",
          action: "network-circuit-open",
          severity: "error",
          dedupeKey: "system:network-circuit-open",
          target: { tab: "settings", section: "proxy" },
          message: {
            title: t("networkCircuitTitle", "Network connection issue"),
            summary: t("networkCircuitMessage", NETWORK_CIRCUIT_MESSAGE),
            detail: t("networkCircuitMessage", NETWORK_CIRCUIT_MESSAGE),
          },
          toast: true,
          toastKind: "error",
        },
      );
    };
    window.addEventListener(NETWORK_CIRCUIT_EVENT, onNetworkCircuitOpen);
    return () => {
      window.removeEventListener(NETWORK_CIRCUIT_EVENT, onNetworkCircuitOpen);
    };
  }, [pushToast, t]);

  useEffect(() => {
    ignoredUpdateVersionRef.current = ignoredUpdateVersion;
  }, [ignoredUpdateVersion]);

  useEffect(() => {
    if (!skillsAutoUpdateNotice) return;
    const timer = window.setTimeout(() => {
      setSkillsAutoUpdateNotice(null);
    }, 4000);
    return () => window.clearTimeout(timer);
  }, [skillsAutoUpdateNotice]);

  useEffect(() => {
    const recordRuntimeMessage = (kind: "error" | "unhandledrejection", text: string) => {
      const firstLine = text.split("\n").find(Boolean) || "Unknown runtime error";
      void notifySystemEvent(
        { pushToast, recordMessage },
        {
          source: "system",
          category: "runtime",
          action: kind,
          severity: "error",
          dedupeKey: `system:runtime:${kind}:${firstLine}`,
          target: { tab: "launcher" },
          message: {
            title:
              kind === "unhandledrejection"
                ? t("runtimeUnhandledRejectionTitle", "Unhandled promise rejection")
                : t("runtimeErrorTitle", "Runtime error"),
            summary: firstLine,
            detail: text,
          },
          toast: true,
          toastKind: "error",
        },
      );
    };

    const handleWindowError = (event: ErrorEvent) => {
      const message =
        event.error?.stack || event.message || "Unknown runtime error";
      console.error("window error", event.error || event.message);
      setRuntimeError(message);
      setRuntimeErrorCopied(false);
      recordRuntimeMessage("error", message);
    };

    const handleUnhandledRejection = (event: PromiseRejectionEvent) => {
      const reason = event.reason;
      const message =
        reason instanceof Error
          ? reason.stack || reason.message
          : typeof reason === "string"
            ? reason
            : JSON.stringify(reason, null, 2);
      console.error("unhandled rejection", reason);
      setRuntimeError(message || "Unhandled promise rejection");
      setRuntimeErrorCopied(false);
      recordRuntimeMessage(
        "unhandledrejection",
        message || "Unhandled promise rejection",
      );
    };

    window.addEventListener("error", handleWindowError);
    window.addEventListener("unhandledrejection", handleUnhandledRejection);
    return () => {
      window.removeEventListener("error", handleWindowError);
      window.removeEventListener(
        "unhandledrejection",
        handleUnhandledRejection,
      );
    };
  }, [pushToast, t]);

  const copyRuntimeError = async () => {
    if (!runtimeError) return;
    try {
      await navigator.clipboard.writeText(runtimeError);
      setRuntimeErrorCopied(true);
      window.setTimeout(() => setRuntimeErrorCopied(false), 2000);
    } catch (error) {
      console.error("failed to copy runtime error", error);
    }
  };

  const handleDragMouseDown = (e: React.MouseEvent<HTMLElement>) => {
    const target = e.target as HTMLElement;
    if (
      target.closest(
        'button,input,select,textarea,a,[role="button"],[data-no-drag]',
      )
    ) {
      return;
    }
    if (!isTauri) return;
    getCurrentWindow()
      .startDragging()
      .catch(() => {});
  };

  const loadCounts = async () => {
    if (!isTauri) return;

    if (loadCountsInFlightRef.current) {
      await loadCountsInFlightRef.current;
      return;
    }

    const task = (async () => {
      try {
        const res = await invoke<ApiResp<DashboardCounts>>("dashboard_counts");
        const data = res.data;
        setCounts((prev) => ({
          ...prev,
          launcher: data.launcher || 0,
          workspaces: data.workspaces || 0,
          sessions: data.sessions || 0,
          ssh: data.ssh || 0,
          snippets: data.snippets || 0,
          bookmarks: data.bookmarks || 0,
          notes: data.notes || 0,
          aiNews: data.ai_news || 0,
          environments: data.environments || 0,
          skills: data.skills || 0,
          subagents: data.subagents || 0,
          mcpServers: data.mcp_servers || 0,
        }));
        if (data.storage_type) {
          setStorageType(data.storage_type);
        }
      } catch (e) {
        console.error("Failed to load local counts", e);
      }

      getUnreadEmailCount()
        .then((mailCount) => {
          setCounts((prev) => ({ ...prev, mail: mailCount }));
        })
        .catch(() => {});
    })();

    loadCountsInFlightRef.current = task.finally(() => {
      loadCountsInFlightRef.current = null;
    });
    await loadCountsInFlightRef.current;
  };

  const scheduleLoadCounts = (delayMs = 180) => {
    if (!isTauri) return;
    if (countsRefreshTimerRef.current !== null) {
      window.clearTimeout(countsRefreshTimerRef.current);
    }
    countsRefreshTimerRef.current = window.setTimeout(() => {
      countsRefreshTimerRef.current = null;
      void loadCounts();
    }, delayMs);
  };

  const refreshMessageUnreadCount = async () => {
    if (!isTauri) return;
    try {
      const count = await getUnreadMessageCount();
      setMessageUnreadCount(count);
    } catch (e) {
      console.error("Failed to load message unread count", e);
    }
  };

  useEffect(() => {
    setMountedTabs((prev) => {
      if (prev.has(activeTab)) return prev;
      const next = new Set(prev);
      next.add(activeTab);
      return next;
    });
  }, [activeTab]);

  useEffect(() => {
    if (!workspaceCapabilityNavigation) return;
    if (activeTab === workspaceCapabilityNavigation.targetTab) return;
    const timer = window.setTimeout(() => {
      setWorkspaceCapabilityNavigation(null);
    }, 0);
    return () => {
      window.clearTimeout(timer);
    };
  }, [activeTab, workspaceCapabilityNavigation]);

  const handleNavigateToCapability = (
    targetTab: CapabilityTargetTab,
    context: WorkspaceCapabilityContext,
  ) => {
    setWorkspaceCapabilityNavigation({ targetTab, context });
    setActiveTab(targetTab);
  };

  const clearWorkspaceCapabilityNavigation = () => {
    setWorkspaceCapabilityNavigation(null);
  };

  // Expose global navigation for components
  useEffect(() => {
    const appWindow = window as AppWindowBindings;
    appWindow.setActiveTab = navigateToTab;
    appWindow.setSettingsOpen = (open: boolean) => {
      if (open) {
        setPreviousTab(activeTab);
        setActiveTab("settings");
      } else {
        setActiveTab(previousTab);
      }
    };
    appWindow.setSettingsTab = setSettingsInitialTab;
  }, [activeTab, navigateToTab, previousTab]);

  useEffect(() => {
    if (!isTauri) {
      setOnboardingStatus("done");
      return;
    }
    invoke<boolean>("should_show_onboarding")
      .then((shouldShow) => {
        setOnboardingStatus(shouldShow ? "required" : "done");
      })
      .catch(() => setOnboardingStatus("done"));
  }, []);

  // Initial load and poll
  useEffect(() => {
    if (isQuickAiView) {
      return;
    }
    if (onboardingStatus !== "done") {
      return;
    }

    const unlistenFns: Array<() => void> = [];
    const addListener = (
      eventName: string,
      handler: (event: { payload?: unknown }) => void,
    ) => {
      listen(eventName, handler)
        .then((fn) => {
          unlistenFns.push(fn);
        })
        .catch((e) => {
          console.error(`Failed to subscribe to ${eventName}`, e);
        });
    };

    if (isTauri) {
      invoke("show_main_window").catch(console.error);
      setTimeout(() => {
        scheduleLoadCounts(0);
      }, 500);
      void refreshMessageUnreadCount();

      setTimeout(() => {
        invoke("sync_run_now").catch((e) => console.error("Sync failed:", e));
      }, 3000);

      addListener("trigger-sync", () => {
        invoke("sync_run_now").catch((e) =>
          console.error("Tray Sync failed:", e),
        );
      });

      addListener("refresh-counts", () => {
        scheduleLoadCounts();
      });

      addListener(MESSAGES_UPDATED_EVENT, (event) => {
        const payload = (event.payload ?? {}) as { unread_count?: number };
        if (typeof payload.unread_count === "number") {
          setMessageUnreadCount(payload.unread_count);
        } else {
          void refreshMessageUnreadCount();
        }
      });

      addListener("ssh-tunnel-connect-failed", (event) => {
        const payload = (event.payload ?? {}) as {
          name?: string;
          error?: string;
        };
        void notifySystemEvent(
          { pushToast, recordMessage },
          buildSshAutoConnectFailedEvent(t, payload),
        );
      });

      addListener("ssh-tunnels-updated", () => {
        const previousErrors = sshTunnelSummaryRef.current?.errorTunnelNames ?? [];
        void invoke<import("./components/sshTunnels/types").SshTunnelsSnapshot>(
          "ssh_tunnels_snapshot",
        )
          .then((snapshot) => {
            const summary = deriveSshTunnelHeaderSummary(snapshot);
            const newErrors = summary.errorTunnelNames.filter(
              (name) => !previousErrors.includes(name),
            );
            for (const name of newErrors) {
              void notifySystemEvent(
                { pushToast, recordMessage },
                buildSshUnexpectedDisconnectEvent(t, name),
              );
            }
            sshTunnelSummaryRef.current = summary;
            setSshTunnelSummary(summary);
          })
          .catch(() => {});
      });

      const refreshProtocolRouterStatus = () => {
        void protocolRouterStatus()
          .then(setProtocolRouterHeaderStatus)
          .catch(() => setProtocolRouterHeaderStatus(null));
      };
      addListener("protocol-router-status-update", refreshProtocolRouterStatus);
      refreshProtocolRouterStatus();

      addListener("ssh-tunnel-window-reconnect-start", (event) => {
        const payload = (event.payload ?? {}) as { total?: number };
        const total = payload.total ?? 0;
        if (total === 0) return;
        if (sshReconnectLoadingToastRef.current) {
          dismissToast(sshReconnectLoadingToastRef.current);
        }
        sshReconnectLoadingToastRef.current = pushToast({
          title: t("sshTunnelReconnecting", "Reconnecting SSH Tunnels"),
          description:
            total > 1
              ? t("sshTunnelReconnectingCount", "Reconnecting {{count}} tunnels…", { count: total })
              : t("sshTunnelReconnectingSingle", "Reconnecting 1 tunnel…"),
          kind: "loading",
        });
      });

      addListener("ssh-tunnel-window-reconnect-done", (event) => {
        const payload = (event.payload ?? {}) as {
          total?: number;
          succeeded?: number;
          failed?: number;
        };
        const descriptor = buildSshWindowReconnectDoneEvent(t, payload);
        if (!descriptor) return;
        if (sshReconnectLoadingToastRef.current) {
          dismissToast(sshReconnectLoadingToastRef.current);
          sshReconnectLoadingToastRef.current = null;
        }
        void notifySystemEvent({ pushToast, recordMessage }, descriptor);
      });

      void invoke<import("./components/sshTunnels/types").SshTunnelsSnapshot>(
        "ssh_tunnels_snapshot",
      )
        .then((snapshot) => {
          const summary = deriveSshTunnelHeaderSummary(snapshot);
          sshTunnelSummaryRef.current = summary;
          setSshTunnelSummary(summary);
        })
        .catch(() => {});

      addListener("refresh-mail-count", () => {
        getUnreadEmailCount()
          .then((mailCount) => {
            setCounts((prev) => ({ ...prev, mail: mailCount }));
          })
          .catch(() => {});
      });

      addListener("tray-action", (event) => {
        const payload = (event.payload ?? {}) as TrayActionPayload;
        if (payload.action !== "navigate" || !payload.target) {
          return;
        }
        const normalizedTarget = normalizeLegacyTabTarget(payload.target);
        if (normalizedTarget === "omni-search") {
          setOmniOpen(true);
          return;
        }
        if (normalizedTarget === "settings") {
          navigateToTab("settings");
          return;
        }
        if (TRAY_NAV_TABS.has(normalizedTarget)) {
          navigateToTab(normalizedTarget);
        }
      });

      addListener("git-sync-status", (event) => {
        const payload = (event.payload ?? {}) as {
          status?: string;
          message?: string;
        };
        const status = payload.status as
          | "pulling"
          | "pushing"
          | "success"
          | "error";
        if (!status) return;
        setSyncStatus(status);
        if (status === "error") {
          setSyncError(payload.message || "Unknown sync error");
        } else {
          setSyncError(null);
        }

        if (status === "success") {
          scheduleLoadCounts(120);
          setTimeout(() => setSyncStatus("idle"), 3000);
        }
      });

      invoke<AppStorageConfig>("get_storage_config")
        .then((cfg) => {
          if (cfg.language) {
            i18n.changeLanguage(cfg.language);
          }
          if (cfg.storage_type) {
            setStorageType(cfg.storage_type);
          }
          setIgnoredUpdateVersion(cfg.update_ignored_version ?? null);
        })
        .catch((e) => console.error("Failed to load language", e));
    }

    let timeoutId: ReturnType<typeof setTimeout> | null = null;
    const pollCounts = async () => {
      await loadCounts();
      timeoutId = setTimeout(pollCounts, 45000);
    };
    pollCounts();

    return () => {
      if (timeoutId) clearTimeout(timeoutId);
      if (countsRefreshTimerRef.current !== null) {
        window.clearTimeout(countsRefreshTimerRef.current);
        countsRefreshTimerRef.current = null;
      }
      unlistenFns.forEach((fn) => fn());
    };
  }, [onboardingStatus, isQuickAiView]);

  useEffect(() => {
    if (!isTauri || onboardingStatus !== "done") {
      return;
    }

    let initialTimer: ReturnType<typeof setTimeout> | null = null;
    let intervalTimer: ReturnType<typeof setInterval> | null = null;
    let cancelled = false;

    const runCheck = async () => {
      if (cancelled) return;
      try {
        const cfg = await invoke<AppStorageConfig>("get_storage_config").catch(
          () => null,
        );
        if (!cfg?.auto_update_enabled) return;
        await checkForUpdates(true, true);
        const current = getUpdaterState();
        const canAutoDownload = current.installable && current.updateAvailable;
        const ignoredVersion = ignoredUpdateVersionRef.current;
        const currentVersion = current.manifest?.version || null;
        const isIgnoredVersion =
          !!ignoredVersion && ignoredVersion === currentVersion;
        if (canAutoDownload && !isIgnoredVersion) {
          await downloadUpdateIfAvailable(true);
        }
      } catch (e) {
        console.error("Auto update scheduler failed:", e);
      }
    };

    const setupScheduler = async () => {
      const cfg = await invoke<AppStorageConfig>("get_storage_config").catch(
        () => null,
      );
      if (!cfg?.auto_update_enabled) return;
      const minsRaw = Number(cfg.update_check_interval_minutes ?? 360);
      const intervalMins = Number.isFinite(minsRaw)
        ? Math.min(1440, Math.max(30, minsRaw))
        : 360;
      initialTimer = setTimeout(runCheck, 20_000);
      intervalTimer = setInterval(runCheck, intervalMins * 60_000);
    };

    setupScheduler();
    return () => {
      cancelled = true;
      if (initialTimer) clearTimeout(initialTimer);
      if (intervalTimer) clearInterval(intervalTimer);
    };
  }, [onboardingStatus, isTauri, checkForUpdates, downloadUpdateIfAvailable]);

  useEffect(() => {
    if (!isTauri || onboardingStatus !== "done") {
      return;
    }
    let intervalTimer: ReturnType<typeof setInterval> | null = null;
    let stopped = false;

    const run = async () => {
      if (stopped) return;
      try {
        const cfg = await invoke<AppStorageConfig>("get_storage_config").catch(
          () => null,
        );
        if (!cfg?.skills_sync_enabled) return;
        await invoke("skills_sync_now");
        if (cfg?.skills_auto_update_enabled) {
          const res = await invoke<ApiResp<RepoAutoUpdateResult>>(
            "skills_repo_auto_update_pending",
          );
          const summary = res?.data;
          if (summary?.updated_repo_count > 0) {
            window.dispatchEvent(
              new CustomEvent(SKILLS_AUTO_UPDATED_EVENT, {
                detail: summary,
              }),
            );
            setSkillsAutoUpdateNotice(
              t(
                "skillsAutoUpdateSummary",
                "Updated {{skills}} skills and synced {{targets}} installed targets.",
                {
                  skills: summary.updated_repo_count,
                  targets: summary.synced_target_count,
                },
              ),
            );
            void notifySystemEvent(
              { pushToast, recordMessage },
              {
                source: "skills",
                category: "auto_update",
                action: "skills-auto-update",
                severity: "success",
                dedupeKey: "skills:auto-update:success",
                target: { tab: "skills" },
                metadata: summary,
                message: {
                  title: t(
                    "skillsAutoUpdateMessageTitle",
                    "Skills auto update completed",
                  ),
                  summary: t(
                    "skillsAutoUpdateMessageSummary",
                    "Updated {{repos}} source(s) and synced {{targets}} installed target(s).",
                    {
                      repos: summary.updated_repo_count,
                      targets: summary.synced_target_count,
                    },
                  ),
                  detail: summary.updated_skill_names.join("\n"),
                },
                toast: false,
              },
            );
          }
        }
      } catch (e) {
        console.error("skills sync scheduler failed", e);
        const text = errorToMessage(e);
        void notifySystemEvent(
          { pushToast, recordMessage },
          {
            source: "skills",
            category: "sync",
            action: "scheduled-sync",
            severity: "error",
            dedupeKey: "skills:scheduled-sync:error",
            target: { tab: "skills" },
            message: {
              title: t("skillsSyncFailedMessageTitle", "Skills source sync failed"),
              summary: text.split("\n").find(Boolean) || "Skills sync failed",
              detail: text,
            },
            toast: true,
            toastKind: "error",
          },
        );
      }
    };

    const setup = async () => {
      const cfg = await invoke<AppStorageConfig>("get_storage_config").catch(
        () => null,
      );
      if (!cfg?.skills_sync_enabled) return;
      const minsRaw = Number(cfg.skills_sync_interval_minutes ?? 60);
      const intervalMins = Number.isFinite(minsRaw)
        ? Math.min(1440, Math.max(5, minsRaw))
        : 60;
      intervalTimer = setInterval(run, intervalMins * 60_000);
    };

    setup();
    return () => {
      stopped = true;
      if (intervalTimer) clearInterval(intervalTimer);
    };
  }, [isTauri, onboardingStatus, t]);

  useEffect(() => {
    if (!isTauri || onboardingStatus !== "done") {
      return;
    }
    let initialTimer: ReturnType<typeof setTimeout> | null = null;
    let intervalTimer: ReturnType<typeof setInterval> | null = null;
    let stopped = false;

    const run = async () => {
      if (stopped) return;
      try {
        const cfg = await invoke<AppStorageConfig>("get_storage_config").catch(
          () => null,
        );
        if (!cfg?.ai_news_enabled) return;
        await invoke("ai_news_sync_now");
      } catch (e) {
        console.error("ai news sync scheduler failed", e);
        const text = errorToMessage(e);
        void notifySystemEvent(
          { pushToast, recordMessage },
          {
            source: "ai_news",
            category: "background_fetch",
            action: "scheduled-sync",
            severity: "error",
            dedupeKey: "ai-news:scheduled-sync:error",
            target: { tab: "ai-news" },
            message: {
              title: t("aiNewsFetchFailedTitle", "AI News fetch failed"),
              summary: text.split("\n").find(Boolean) || "AI News sync failed",
              detail: text,
            },
            toast: true,
            toastKind: "error",
          },
        );
      }
    };

    const setup = async () => {
      const cfg = await invoke<AppStorageConfig>("get_storage_config").catch(
        () => null,
      );
      if (!cfg?.ai_news_enabled) return;
      const minsRaw = Number(cfg.ai_news_sync_interval_minutes ?? 60);
      const intervalMins = Number.isFinite(minsRaw)
        ? Math.min(1440, Math.max(5, minsRaw))
        : 60;
      initialTimer = setTimeout(() => {
        void run();
      }, 15_000);
      intervalTimer = setInterval(run, intervalMins * 60_000);
    };

    setup();
    return () => {
      stopped = true;
      if (initialTimer) clearTimeout(initialTimer);
      if (intervalTimer) clearInterval(intervalTimer);
    };
  }, [isTauri, onboardingStatus]);

  useEffect(() => {
    if (!isTauri || onboardingStatus !== "done") {
      return;
    }
    let intervalTimer: ReturnType<typeof setInterval> | null = null;
    let stopped = false;

    const run = async () => {
      if (stopped) return;
      try {
        const cfg = await invoke<AppStorageConfig>("get_storage_config").catch(
          () => null,
        );
        if (!cfg?.subagents_sync_enabled) return;
        await invoke("subagents_sync_now");
      } catch (e) {
        console.error("subagents sync scheduler failed", e);
        const text = errorToMessage(e);
        void notifySystemEvent(
          { pushToast, recordMessage },
          {
            source: "subagents",
            category: "sync",
            action: "scheduled-sync",
            severity: "error",
            dedupeKey: "subagents:scheduled-sync:error",
            target: { tab: "subagents" },
            message: {
              title: t(
                "subagentsSyncFailedMessageTitle",
                "Subagents source sync failed",
              ),
              summary: text.split("\n").find(Boolean) || "Subagents sync failed",
              detail: text,
            },
            toast: true,
            toastKind: "error",
          },
        );
      }
    };

    const setup = async () => {
      const cfg = await invoke<AppStorageConfig>("get_storage_config").catch(
        () => null,
      );
      if (!cfg?.subagents_sync_enabled) return;
      const minsRaw = Number(cfg.subagents_sync_interval_minutes ?? 60);
      const intervalMins = Number.isFinite(minsRaw)
        ? Math.min(1440, Math.max(5, minsRaw))
        : 60;
      intervalTimer = setInterval(run, intervalMins * 60_000);
    };

    setup();
    return () => {
      stopped = true;
      if (intervalTimer) clearInterval(intervalTimer);
    };
  }, [isTauri, onboardingStatus]);

  const navigationGroups = useMemo(
    () => [
      {
        id: "core",
        label: i18n.language === "zh" ? "核心" : "Core",
        items: [
          {
            id: "launcher",
            name: t("launcher"),
            icon: Rocket,
            count: counts.launcher,
          },
          {
            id: "workspaces",
            name: t("workspaces", "Workspaces"),
            icon: FolderOpen,
            count: counts.workspaces,
          },
          {
            id: "ai-assistants",
            name: smartWorkspaceLabel,
            icon: Bot,
          },
          {
            id: "ai-sessions",
            name: t("aiSessions"),
            icon: Terminal,
            count: counts.sessions,
          },
        ],
      },
      {
        id: "capabilities",
        label: i18n.language === "zh" ? "AI 能力" : "AI Capabilities",
        items: [
          {
            id: "ai-environments",
            name: t("cliEnvironments", "AI Terminal Environments"),
            icon: Cpu,
            count: counts.environments,
          },
          {
            id: "ai-usage",
            name: t("aiUsageStatsMenu", "AI Usage Stats"),
            icon: BarChart3,
          },
          {
            id: "ai-flow",
            name: "AI Flow",
            icon: Waypoints,
          },
          {
            id: "skills",
            name: t("skills", "Skills"),
            icon: Sparkles,
            count: counts.skills,
          },
          {
            id: "mcp-servers",
            name: "MCP Servers",
            icon: MCPIcon,
            count: counts.mcpServers,
          },
          {
            id: "subagents",
            name: t("subagents", "Subagents"),
            icon: Bot,
            count: counts.subagents,
          },
        ],
      },
      {
        id: "tools",
        label: i18n.language === "zh" ? "工具" : "Tools",
        items: [
          {
            id: "snippets",
            name: t("snippets", "Snippets"),
            icon: Code2,
            count: counts.snippets,
          },
          {
            id: "notes",
            name: t("notes", "Notes"),
            icon: NotebookPen,
            count: counts.notes,
          },
          {
            id: "more-tools",
            name: moreToolsLabel,
            icon: Rocket,
          },
        ],
      },
    ],
    [counts, i18n.language, moreToolsLabel, smartWorkspaceLabel, t],
  );

  const isNavigationItemActive = (itemId: string) => {
    if (itemId === "ai-assistants") {
      return isSmartWorkspaceTab(activeTab);
    }
    if (itemId === "more-tools") {
      return isMoreToolsTab(activeTab);
    }
    return activeTab === itemId;
  };

  const toggleLanguage = async () => {
    const newLang = i18n.language === "zh" ? "en" : "zh";
    await i18n.changeLanguage(newLang);

    if (isTauri) {
      try {
        const cfg = await invoke<AppStorageConfig>("get_storage_config");
        await invoke("save_storage_config", {
          config: { ...cfg, language: newLang },
        });
        await invoke("update_tray_menu", { lang: newLang });
      } catch (e) {
        console.error("Failed to save language preference:", e);
      }
    }
  };

  const cycleTheme = () => {
    if (theme === "system") setTheme("dark");
    else if (theme === "dark") setTheme("light");
    else setTheme("system");
  };

  const openGithubRepo = async () => {
    const repoUrl = "https://github.com/minbox-projects/one-space";
    await openExternalUrl(repoUrl);
  };

  const copySyncError = () => {
    if (syncError) {
      navigator.clipboard.writeText(syncError);
      const originalError = syncError;
      setSyncError(t("copied", "Copied!"));
      setTimeout(() => setSyncError(originalError), 2000);
    }
  };

  const ThemeIcon =
    theme === "system" ? Monitor : theme === "dark" ? Moon : Sun;
  const themeLabel =
    theme === "system"
      ? t("themeSystem")
      : theme === "dark"
        ? t("themeDark")
        : t("themeLight");
  const currentUpdateVersion = updaterManifest?.version ?? "";
  const hasUpdateCandidate =
    !!currentUpdateVersion &&
    (updaterStatus === "available" ||
      updaterStatus === "downloading" ||
      updaterStatus === "downloaded" ||
      updaterStatus === "installing");
  const isCurrentVersionIgnored =
    !!currentUpdateVersion && currentUpdateVersion === ignoredUpdateVersion;
  const showUpdateIndicator = hasUpdateCandidate && !isCurrentVersionIgnored;
  const updateIndicatorTitle = t("newVersionDetected", {
    version: currentUpdateVersion,
  });

  useEffect(() => {
    if (!showUpdateIndicator) {
      setUpdateDialogOpen(false);
    }
  }, [showUpdateIndicator]);

  useEffect(() => {
    if (!isTauri || onboardingStatus !== "done") return;
    const event = buildUpdaterSystemEvent(t, {
      version: updaterManifest?.version,
      currentVersion: updaterManifest?.currentVersion,
      body: updaterManifest?.body,
      status: updaterStatus,
      error: updaterError,
      showUpdateIndicator,
      source: getUpdaterState().source,
    });
    const key = event?.key ?? null;
    const descriptor = event?.descriptor ?? null;
    if (!key || !descriptor || updateMessageKeyRef.current === key) return;
    updateMessageKeyRef.current = key;
    void notifySystemEvent({ pushToast, recordMessage }, descriptor);
  }, [
    isTauri,
    onboardingStatus,
    showUpdateIndicator,
    updaterError,
    updaterManifest,
    updaterStatus,
    pushToast,
    t,
  ]);

  const openReleasesPage = async () => {
    const releasesUrl = "https://github.com/minbox-projects/one-space/releases";
    await openExternalUrl(releasesUrl);
  };

  const handleUpgradeNow = async () => {
    if (!updaterManifest?.version) {
      return;
    }
    try {
      if (!updaterInstallable) {
        await openReleasesPage();
        return;
      }
      if (updaterStatus === "downloaded") {
        await installDownloadedUpdate();
        return;
      }
      await installUpdate();
      const current = getUpdaterState();
      if (current.status === "downloaded") {
        await installDownloadedUpdate();
      }
    } catch (e) {
      console.error("Failed to run update flow:", e);
    }
  };

  useEffect(() => {
    if (!isTauri || onboardingStatus !== "done") return;
    if (
      (updaterStatus === "available" || updaterStatus === "downloaded") &&
      showUpdateIndicator
    ) {
      invoke<AppStorageConfig>("get_storage_config")
        .then((cfg) => {
          if (cfg?.auto_update_enabled) {
            setAutoUpdateNoticeOpen(true);
            if (updaterStatus === "available") {
              void downloadUpdateIfAvailable(true);
            }
          }
        })
        .catch(console.error);
    }
  }, [
    updaterStatus,
    showUpdateIndicator,
    isTauri,
    onboardingStatus,
    downloadUpdateIfAvailable,
  ]);

  useEffect(() => {
    if (updateDialogOpen && autoUpdateNoticeOpen) {
      setAutoUpdateNoticeOpen(false);
    }
  }, [updateDialogOpen, autoUpdateNoticeOpen]);

  const handleIgnoreVersion = async () => {
    if (!updaterManifest?.version) {
      return;
    }
    try {
      const cfg = await invoke<AppStorageConfig>("get_storage_config");
      await invoke("save_storage_config", {
        config: {
          ...cfg,
          update_ignored_version: updaterManifest.version,
        },
      });
      setIgnoredUpdateVersion(updaterManifest.version);
      setUpdateDialogOpen(false);
    } catch (e) {
      console.error("Failed to save ignored update version:", e);
    }
  };

  const renderUpdateIndicatorIcon = () => {
    if (updaterStatus === "downloading" || updaterStatus === "installing") {
      return <Loader2 className="w-5 h-5 animate-spin" />;
    }
    if (updaterStatus === "downloaded") {
      return <ArrowUpCircle className="w-5 h-5 animate-pulse" />;
    }
    return <ArrowUpCircle className="w-5 h-5 animate-bounce" />;
  };

  const toggleFishPond = () => {
    if (activeTab === "fish-pond") {
      const fallbackTab =
        fishPondPreviousTab !== "fish-pond" ? fishPondPreviousTab : "launcher";
      setActiveTab(fallbackTab);
      return;
    }
    setFishPondPreviousTab(activeTab);
    setActiveTab("fish-pond");
  };

  const resolvedTheme = useMemo(
    () =>
      theme === "system"
        ? window.matchMedia("(prefers-color-scheme: dark)").matches
          ? "dark"
          : "light"
        : theme,
    [theme],
  );

  // If we are in quick-ai view, render only that component
  if (isQuickAiView) {
    return <QuickAiSessionBar />;
  }

  if (isQuickAssistantView) {
    return <QuickAssistantWindow />;
  }

  if (isSelectionAssistantView) {
    return <QuickAssistantWindow variant="selection" />;
  }

  if (onboardingStatus === "checking") {
    return (
      <div className="h-screen w-screen bg-background text-foreground flex items-center justify-center">
        <div className="text-sm text-muted-foreground">
          {t("loading", "Loading...")}
        </div>
      </div>
    );
  }

  if (onboardingStatus === "required") {
    return (
      <OnboardingWizard
        onComplete={(nextStorageType) => {
          setStorageType(nextStorageType);
          setOnboardingStatus("done");
        }}
      />
    );
  }

  const renderContent = () => {
    const shouldRenderTab = (tabId: string) =>
      activeTab === tabId || mountedTabs.has(tabId);
    return (
      <div className="h-full relative">
        {shouldRenderTab("launcher") && (
          <div className={activeTab === "launcher" ? "h-full" : "hidden"}>
            <Launcher isVisible={activeTab === "launcher"} />
          </div>
        )}
        {shouldRenderTab("workspaces") && (
          <div className={activeTab === "workspaces" ? "h-full" : "hidden"}>
            <AppErrorBoundary label="工作空间" resetKey={activeTab}>
              <Workspaces
                isVisible={activeTab === "workspaces"}
                onNavigateToCapability={handleNavigateToCapability}
              />
            </AppErrorBoundary>
          </div>
        )}
        {shouldRenderTab("ai-sessions") && (
          <div className={activeTab === "ai-sessions" ? "h-full" : "hidden"}>
            <AiSessions
              isVisible={activeTab === "ai-sessions"}
              onNavigate={(tab, hash) => {
                navigateToTab(tab);
                if (hash) window.location.hash = hash;
              }}
            />
          </div>
        )}
        {shouldRenderTab("ai-assistants") && (
          <div className={activeTab === "ai-assistants" ? "h-full" : "hidden"}>
            <SmartWorkspaceHub
              initialSection={smartWorkspaceSection}
            />
          </div>
        )}
        {shouldRenderTab("ai-environments") && (
          <div
            className={activeTab === "ai-environments" ? "h-full" : "hidden"}
          >
            <AiEnvironments isVisible={activeTab === "ai-environments"} />
          </div>
        )}
        {shouldRenderTab("ai-usage") && (
          <div className={activeTab === "ai-usage" ? "h-full" : "hidden"}>
            <AiUsageStats isVisible={activeTab === "ai-usage"} />
          </div>
        )}
        {shouldRenderTab("ai-flow") && (
          <div className={activeTab === "ai-flow" ? "h-full" : "hidden"}>
            <AiFlow isVisible={activeTab === "ai-flow"} />
          </div>
        )}
        {shouldRenderTab("ai-news") && (
          <div className={activeTab === "ai-news" ? "h-full" : "hidden"}>
            <AiNews isVisible={activeTab === "ai-news"} />
          </div>
        )}
        {shouldRenderTab("skills") && (
          <div className={activeTab === "skills" ? "h-full" : "hidden"}>
            <Skills
              isVisible={activeTab === "skills"}
              initialEntry={
                workspaceCapabilityNavigation?.targetTab === "skills"
                  ? workspaceCapabilityNavigation.context.entry
                  : undefined
              }
              onConsumeInitialEntry={clearWorkspaceCapabilityNavigation}
            />
          </div>
        )}
        {shouldRenderTab("subagents") && (
          <div className={activeTab === "subagents" ? "h-full" : "hidden"}>
            <Subagents
              isVisible={activeTab === "subagents"}
              initialEntry={
                workspaceCapabilityNavigation?.targetTab === "subagents"
                  ? workspaceCapabilityNavigation.context.entry
                  : undefined
              }
              onConsumeInitialEntry={clearWorkspaceCapabilityNavigation}
            />
          </div>
        )}
        {shouldRenderTab("mcp-servers") && (
          <div className={activeTab === "mcp-servers" ? "h-full" : "hidden"}>
            <MCPServers
              isVisible={activeTab === "mcp-servers"}
              workspaceContext={
                workspaceCapabilityNavigation?.targetTab === "mcp-servers"
                  ? workspaceCapabilityNavigation.context
                  : undefined
              }
              onDismissWorkspaceContext={clearWorkspaceCapabilityNavigation}
            />
          </div>
        )}
        {shouldRenderTab("snippets") && (
          <div className={activeTab === "snippets" ? "h-full" : "hidden"}>
            <Snippets />
          </div>
        )}
        {shouldRenderTab("notes") && (
          <div className={activeTab === "notes" ? "h-full" : "hidden"}>
            <Notes />
          </div>
        )}
        {shouldRenderTab("more-tools") && (
          <div className={activeTab === "more-tools" ? "h-full" : "hidden"}>
            <MoreToolsHub
              activeTool={moreToolsSection}
              onSelectTool={setMoreToolsSection}
            />
          </div>
        )}
        {shouldRenderTab("documentation") && (
          <div className={activeTab === "documentation" ? "h-full" : "hidden"}>
            <Documentation />
          </div>
        )}
        {shouldRenderTab("mail") && (
          <div className={activeTab === "mail" ? "h-full" : "hidden"}>
            <Mail isVisible={activeTab === "mail"} />
          </div>
        )}
        {shouldRenderTab("fish-pond") && (
          <div className={activeTab === "fish-pond" ? "h-full" : "hidden"}>
            <FishPond />
          </div>
        )}
        {shouldRenderTab("settings") && (
          <div className={activeTab === "settings" ? "h-full" : "hidden"}>
            <SettingsView
              initialTab={settingsInitialTab}
              onBack={() => {
                setActiveTab(previousTab);
                setSettingsInitialTab("storage");
                scheduleLoadCounts(0);
              }}
            />
          </div>
        )}
      </div>
    );
  };

  return (
    <div className="flex h-screen bg-background text-foreground overflow-hidden select-none">
      <div className="w-64 border-r bg-muted/20 flex flex-col">
        <div
          className="h-16 flex items-end pl-5 pr-4 pb-1.5 border-b font-semibold tracking-tight cursor-default select-none relative"
          data-tauri-drag-region
          onMouseDown={handleDragMouseDown}
        >
          <div className="flex items-center gap-2 pointer-events-none">
            <img
              src={resolvedTheme === "dark" ? logoWhite : logoBlack}
              alt="OneSpace"
              className="w-5 h-5"
            />
            <span className="text-lg">OneSpace</span>
          </div>
        </div>

        <div className="flex-1 overflow-y-auto py-4 px-3 space-y-5">
          {navigationGroups.map((group) => (
            <div key={group.id} className="space-y-1.5">
              <div className="px-3 text-[11px] font-semibold uppercase tracking-[0.18em] text-muted-foreground">
                {group.label}
              </div>
              {group.items.map((item: any) => {
                const selected = isNavigationItemActive(item.id);
                return (
                  <button
                    key={item.id}
                    onClick={() => navigateToTab(item.id)}
                    className={`w-full flex items-center justify-between px-3 py-2 rounded-md text-sm transition-colors ${
                      selected
                        ? "bg-primary text-primary-foreground font-medium shadow-sm"
                        : "hover:bg-muted text-muted-foreground hover:text-foreground"
                    }`}
                  >
                    <div className="flex items-center gap-3">
                      <item.icon
                        className={`w-4 h-4 ${selected ? "animate-pulse" : ""}`}
                      />
                      <span>{item.name}</span>
                      {item.id === "ai-assistants" && (
                        <span className="text-[9px] px-1 py-0.5 rounded bg-orange-500/10 text-orange-600 dark:text-orange-400 font-medium uppercase tracking-wide">
                          beta
                        </span>
                      )}
                    </div>
                    {item.count !== undefined && (
                      <span
                        className={`text-[10px] px-1.5 py-0.5 rounded-full font-mono ${
                          selected
                            ? "bg-primary-foreground/20 text-primary-foreground"
                            : "bg-muted-foreground/10 text-muted-foreground"
                        }`}
                      >
                        {item.count}
                      </span>
                    )}
                  </button>
                );
              })}
            </div>
          ))}
        </div>

        <div className="p-3 border-t space-y-1">
          <button
            onClick={() => {
              setPreviousTab(activeTab);
              setActiveTab("settings");
            }}
            className={`w-full flex items-center gap-3 px-3 py-2 rounded-md text-sm transition-colors ${
              activeTab === "settings"
                ? "bg-primary/10 text-primary font-medium"
                : "text-muted-foreground hover:bg-muted hover:text-foreground"
            }`}
          >
            <Settings
              className={`w-4 h-4 ${activeTab === "settings" ? "animate-pulse" : ""}`}
            />
            {t("settings")}
          </button>
          <button
            onClick={() => navigateToTab("documentation")}
            className={`w-full flex items-center gap-3 px-3 py-2 rounded-md text-sm transition-colors ${
              activeTab === "documentation"
                ? "bg-primary/10 text-primary font-medium"
                : "text-muted-foreground hover:bg-muted hover:text-foreground"
            }`}
          >
            <BookOpen
              className={`w-4 h-4 ${activeTab === "documentation" ? "animate-pulse" : ""}`}
            />
            {t("usageDocs")}
          </button>
          <button
            onClick={() => setAboutOpen(true)}
            className="w-full flex items-center gap-3 px-3 py-2 rounded-md text-sm text-muted-foreground hover:bg-muted hover:text-foreground transition-colors"
          >
            <Info className="w-4 h-4" />
            {t("about")}
          </button>
        </div>
      </div>

      <div className="flex-1 flex flex-col overflow-hidden relative bg-background">
        {activeTab !== "settings" && (
          <header
            className="h-16 border-b flex items-end px-6 pb-1.5 justify-between bg-background/95 backdrop-blur supports-[backdrop-filter]:bg-background/60 relative"
            data-tauri-drag-region
            onMouseDown={handleDragMouseDown}
          >
            <div className="flex-1 flex items-center gap-4">
              <button
                onClick={() => setOmniOpen(true)}
                className="flex items-center justify-between w-full max-w-[320px] px-3 py-1.5 text-sm text-muted-foreground bg-muted/40 hover:bg-muted/60 rounded-lg border border-border/50 transition-all shadow-sm group"
              >
                <div className="flex items-center gap-2.5">
                  <Search className="w-4 h-4 text-muted-foreground/70 group-hover:text-foreground transition-colors" />
                  <span className="group-hover:text-foreground transition-colors">
                    {t("search")}...
                  </span>
                </div>
                <kbd className="hidden sm:inline-flex h-5 items-center gap-1 rounded border bg-background/50 px-1.5 font-mono text-[10px] font-medium opacity-60">
                  <span className="text-xs">⌘</span>K
                </kbd>
              </button>

              {syncStatus !== "idle" && (
                <div className="flex items-center gap-2">
                  {syncStatus === "pulling" && (
                    <div className="flex items-center gap-2 px-2.5 py-1 bg-primary/5 rounded-full border border-primary/10 animate-pulse">
                      <Loader2 className="w-3 h-3 text-primary animate-spin" />
                      <span className="text-[10px] font-semibold text-primary/80 uppercase tracking-wider">
                        {storageType === "git"
                          ? t("syncingToGit", "Syncing to Git")
                          : storageType === "icloud"
                            ? t("savingToICloud", "Syncing to iCloud")
                            : t("savingLocally")}
                      </span>
                    </div>
                  )}
                  {syncStatus === "pushing" && (
                    <div className="flex items-center gap-2 px-2.5 py-1 bg-primary/5 rounded-full border border-primary/10 animate-pulse">
                      <Loader2 className="w-3 h-3 text-primary animate-spin" />
                      <span className="text-[10px] font-semibold text-primary/80 uppercase tracking-wider">
                        {storageType === "git"
                          ? t("syncingToGit", "Syncing to Git")
                          : storageType === "icloud"
                            ? t("savingToICloud", "Syncing to iCloud")
                            : t("savingLocally")}
                      </span>
                    </div>
                  )}
                  {syncStatus === "success" && (
                    <div className="flex items-center gap-2 px-2.5 py-1 bg-green-500/5 rounded-full border border-green-500/20">
                      <CheckCircle2 className="w-3 h-3 text-green-500" />
                      <span className="text-[10px] font-semibold text-green-500/80 uppercase tracking-wider">
                        {storageType === "git"
                          ? t("syncedToGit")
                          : storageType === "icloud"
                            ? t("savedToICloud", "Saved to iCloud")
                            : t("savedLocally")}
                      </span>
                    </div>
                  )}
                  {syncStatus === "error" && (
                    <div
                      className="group relative flex items-center gap-2 px-2.5 py-1 bg-destructive/5 rounded-full border border-destructive/20 cursor-pointer transition-colors hover:bg-destructive/10"
                      onClick={copySyncError}
                    >
                      <AlertCircle className="w-3 h-3 text-destructive" />
                      <span className="text-[10px] font-semibold text-destructive/80 uppercase tracking-wider">
                        {t("syncError", "Sync Error")}
                      </span>
                      <div className="absolute left-0 top-full mt-2 w-64 p-2 bg-destructive text-destructive-foreground text-[10px] rounded-md shadow-xl opacity-0 group-hover:opacity-100 transition-opacity z-50 select-text pointer-events-auto border border-destructive/20">
                        <div className="flex flex-col gap-1">
                          <span className="font-bold border-b border-destructive-foreground/20 pb-1 mb-1 flex justify-between items-center">
                            {t("syncErrorInfo", "Error Details")}
                            <span className="text-[8px] opacity-70 uppercase tracking-widest bg-destructive-foreground/10 px-1 rounded">
                              {t("clickToCopy", "Click to copy")}
                            </span>
                          </span>
                          <span className="break-words line-clamp-4 leading-relaxed">
                            {syncError}
                          </span>
                        </div>
                      </div>
                    </div>
                  )}
                </div>
              )}
            </div>

            <div className="flex items-center gap-1">
              {protocolRouterHeaderStatus?.enabled && (
                <button
                  onClick={() => navigateToTab("protocol-router")}
                  className={`relative p-2.5 rounded-md transition-colors ${
                    protocolRouterHeaderStatus.running
                      ? "text-emerald-600 hover:bg-emerald-500/10"
                      : "text-amber-600 hover:bg-amber-500/10"
                  }`}
                  title={
                    protocolRouterHeaderStatus.running
                      ? t("launcherProtocolRouterRunningAria", {
                          port: protocolRouterHeaderStatus.port,
                          routes: protocolRouterHeaderStatus.route_count,
                          defaultValue: `Protocol router running on port ${protocolRouterHeaderStatus.port} with ${protocolRouterHeaderStatus.route_count} route(s)`,
                        })
                      : t("launcherProtocolRouterStoppedAria", {
                          port: protocolRouterHeaderStatus.port,
                          defaultValue: `Protocol router is enabled but stopped on port ${protocolRouterHeaderStatus.port}`,
                        })
                  }
                  aria-label={
                    protocolRouterHeaderStatus.running
                      ? t("launcherProtocolRouterRunningAria", {
                          port: protocolRouterHeaderStatus.port,
                          routes: protocolRouterHeaderStatus.route_count,
                          defaultValue: `Protocol router running on port ${protocolRouterHeaderStatus.port} with ${protocolRouterHeaderStatus.route_count} route(s)`,
                        })
                      : t("launcherProtocolRouterStoppedAria", {
                          port: protocolRouterHeaderStatus.port,
                          defaultValue: `Protocol router is enabled but stopped on port ${protocolRouterHeaderStatus.port}`,
                        })
                  }
                >
                  <Route className="w-5 h-5" />
                  <span
                    className={`absolute -right-0.5 -top-0.5 min-w-5 rounded-full px-1.5 py-0.5 text-[10px] font-semibold text-white ${
                      protocolRouterHeaderStatus.running ? "bg-emerald-500" : "bg-amber-500"
                    }`}
                  >
                    {protocolRouterHeaderStatus.route_count > 99
                      ? "99+"
                      : protocolRouterHeaderStatus.route_count}
                  </span>
                </button>
              )}
              {sshTunnelSummary && sshTunnelSummary.connectedCount > 0 && (
                <button
                  onClick={() => navigateToTab("ssh-tunnels")}
                  className={`relative p-2.5 rounded-md transition-colors ${
                    sshTunnelSummary.hasErrors
                      ? "text-destructive hover:bg-destructive/10"
                      : sshTunnelSummary.hasConnecting
                        ? "text-blue-500 hover:bg-blue-500/10"
                        : "text-muted-foreground hover:bg-muted hover:text-foreground"
                  }`}
                  title={
                    sshTunnelSummary.hasErrors
                      ? `${sshTunnelSummary.connectedCount} connected, ${sshTunnelSummary.errorTunnelNames.join(", ")} disconnected`
                      : `${sshTunnelSummary.connectedCount} SSH tunnel${sshTunnelSummary.connectedCount > 1 ? "s" : ""} connected`
                  }
                >
                  <Waypoints className="w-5 h-5" />
                  {sshTunnelSummary.hasErrors && (
                    <span className="absolute -right-0.5 -top-0.5 min-w-5 rounded-full bg-destructive px-1.5 py-0.5 text-[10px] font-semibold text-destructive-foreground animate-pulse">
                      {sshTunnelSummary.connectedCount > 99
                        ? "99+"
                        : sshTunnelSummary.connectedCount}
                    </span>
                  )}
                  {sshTunnelSummary.hasConnecting && !sshTunnelSummary.hasErrors && (
                    <span className="absolute -right-0.5 -top-0.5 min-w-5 rounded-full bg-blue-500 px-1.5 py-0.5 text-[10px] font-semibold text-white animate-pulse">
                      {sshTunnelSummary.connectedCount > 99
                        ? "99+"
                        : sshTunnelSummary.connectedCount}
                    </span>
                  )}
                  {!sshTunnelSummary.hasErrors && !sshTunnelSummary.hasConnecting && (
                    <span className="absolute -right-0.5 -top-0.5 min-w-5 rounded-full bg-emerald-500 px-1.5 py-0.5 text-[10px] font-semibold text-white">
                      {sshTunnelSummary.connectedCount > 99
                        ? "99+"
                        : sshTunnelSummary.connectedCount}
                    </span>
                  )}
                </button>
              )}
              <button
                onClick={() => setMessageCenterOpen(true)}
                className={`relative p-2.5 rounded-md transition-colors ${
                  messageCenterOpen
                    ? "bg-primary/10 text-primary"
                    : "text-muted-foreground hover:bg-muted hover:text-foreground"
                }`}
                title={t("messageCenter", "消息中心")}
              >
                <Bell className="w-5 h-5" />
                {messageUnreadCount > 0 ? (
                  <span className="absolute -right-0.5 -top-0.5 min-w-5 rounded-full bg-destructive px-1.5 py-0.5 text-[10px] font-semibold text-destructive-foreground">
                    {messageUnreadCount > 99 ? "99+" : messageUnreadCount}
                  </span>
                ) : null}
              </button>

              <button
                onClick={() => {
                  if (activeTab === "mail") {
                    setActiveTab(previousTab);
                  } else {
                    setPreviousTab(activeTab);
                    navigateToTab("mail");
                  }
                }}
                className={`relative p-2.5 rounded-md transition-colors ${
                  activeTab === "mail"
                    ? "bg-primary/10 text-primary"
                    : "text-muted-foreground hover:bg-muted hover:text-foreground"
                }`}
                title={t("mail", "Mail")}
              >
                <MailIcon className="w-5 h-5" />
                {counts.mail > 0 ? (
                  <span className="absolute -right-0.5 -top-0.5 min-w-5 rounded-full bg-primary px-1.5 py-0.5 text-[10px] font-semibold text-primary-foreground">
                    {counts.mail > 99 ? "99+" : counts.mail}
                  </span>
                ) : null}
              </button>

              <button
                onClick={() => {
                  if (activeTab === "ai-news") {
                    setActiveTab(previousTab);
                  } else {
                    setPreviousTab(activeTab);
                    navigateToTab("ai-news");
                  }
                }}
                className={`relative p-2.5 rounded-md transition-colors ${
                  activeTab === "ai-news"
                    ? "bg-primary/10 text-primary"
                    : "text-muted-foreground hover:bg-muted hover:text-foreground"
                }`}
                title={t("aiNews", "AI News")}
              >
                <Newspaper className="w-5 h-5" />
                {counts.aiNews > 0 ? (
                  <span className="absolute right-1 top-1 h-2.5 w-2.5 rounded-full bg-primary" />
                ) : null}
              </button>

              <button
                onClick={toggleFishPond}
                className={`p-2.5 rounded-md transition-colors ${
                  activeTab === "fish-pond"
                    ? "bg-primary/10 text-primary"
                    : "text-muted-foreground hover:bg-muted hover:text-foreground"
                }`}
                title={t("fishPond", "Fish Pond")}
              >
                <Fish className="w-5 h-5" />
              </button>

              <button
                onClick={openGithubRepo}
                className="p-2.5 text-muted-foreground hover:bg-muted hover:text-foreground rounded-md transition-colors"
                title="GitHub"
              >
                <span className="inline-flex h-5 w-5 items-center justify-center rounded-full bg-black">
                  <Github className="w-3.5 h-3.5 text-white" />
                </span>
              </button>

              <button
                onClick={toggleLanguage}
                className="p-2.5 text-muted-foreground hover:bg-muted hover:text-foreground rounded-md transition-colors"
                title={t("toggleLanguage")}
              >
                {i18n.language === "zh" ? (
                  <span className="text-sm font-bold font-mono">EN</span>
                ) : (
                  <span className="text-sm font-bold">中</span>
                )}
              </button>

              <button
                onClick={cycleTheme}
                className="p-2.5 text-muted-foreground hover:bg-muted hover:text-foreground rounded-md transition-colors"
                title={themeLabel}
              >
                <ThemeIcon className="w-5 h-5" />
              </button>

              {showUpdateIndicator && (
                <button
                  onClick={() => setUpdateDialogOpen(true)}
                  className="p-2.5 text-primary hover:bg-primary/10 rounded-md transition-colors"
                  title={updateIndicatorTitle}
                >
                  {renderUpdateIndicatorIcon()}
                </button>
              )}
            </div>
          </header>
        )}

        <main
          className={`flex-1 overflow-y-auto ${activeTab === "settings" ? "p-0" : "p-6"}`}
        >
          {renderContent()}
        </main>
      </div>

      {autoUpdateNoticeOpen && showUpdateIndicator && (
        <div className="fixed right-4 top-4 z-[121] w-80">
          <div className="overflow-hidden rounded-xl border bg-card shadow-2xl">
            <div className="p-4">
              <div className="flex items-start justify-between gap-3">
                <div className="flex items-center gap-2">
                  <div className="flex h-8 w-8 items-center justify-center rounded-lg bg-primary/10 text-primary">
                    <ArrowUpCircle className="h-5 w-5" />
                  </div>
                  <div>
                    <h4 className="text-sm font-semibold text-foreground">
                      {t("updateAvailableTitle")}
                    </h4>
                    <p className="text-xs text-muted-foreground">
                      v{updaterManifest?.version}
                    </p>
                  </div>
                </div>
                <button
                  type="button"
                  onClick={() => setAutoUpdateNoticeOpen(false)}
                  className="rounded-md p-1 text-muted-foreground hover:bg-muted hover:text-foreground transition-colors"
                >
                  <X className="h-4 w-4" />
                </button>
              </div>

              <div className="mt-4 space-y-3">
                {(updaterStatus === "downloading" ||
                  updaterStatus === "available") && (
                  <div className="space-y-1.5">
                    <div className="flex items-center justify-between text-[10px] text-muted-foreground uppercase tracking-wider font-medium">
                      <span>{t("updateDownloading")}</span>
                      <span>{updaterDownloadProgress}%</span>
                    </div>
                    <div className="h-1.5 w-full overflow-hidden rounded-full bg-muted">
                      <div
                        className="h-full bg-primary transition-all duration-300 ease-out"
                        style={{ width: `${updaterDownloadProgress}%` }}
                      />
                    </div>
                  </div>
                )}

                {updaterStatus === "downloaded" && (
                  <div className="space-y-3">
                    <p className="text-xs text-muted-foreground leading-relaxed">
                      {t("updateDownloadedReady")}
                    </p>
                    <button
                      type="button"
                      onClick={() => void installDownloadedUpdate()}
                      className="flex w-full items-center justify-center gap-2 rounded-lg bg-primary py-2 text-sm font-semibold text-primary-foreground shadow-sm transition-all hover:opacity-90 active:scale-[0.98]"
                    >
                      <Rocket className="h-4 w-4" />
                      {t("restartToInstall")}
                    </button>
                  </div>
                )}
              </div>
            </div>
          </div>
        </div>
      )}

      {networkCircuitOpen && (
        <div className="fixed right-4 top-4 z-[120] max-w-md">
          <div className="flex items-start gap-2 rounded-lg border border-destructive/40 bg-destructive px-3 py-2 text-destructive-foreground shadow-lg">
            <AlertCircle className="mt-0.5 h-4 w-4 shrink-0" />
            <p className="text-sm leading-5">
              {t("networkCircuitMessage", NETWORK_CIRCUIT_MESSAGE)}
            </p>
            <button
              type="button"
              className="shrink-0 rounded-sm p-0.5 transition-colors hover:bg-white/20"
              aria-label={t("close", "Close")}
              onClick={() => setNetworkCircuitOpen(false)}
            >
              <X className="h-4 w-4" />
            </button>
          </div>
        </div>
      )}

      {skillsAutoUpdateNotice && (
        <div className="fixed right-4 top-20 z-[119] max-w-md">
          <div className="flex items-start gap-2 rounded-lg border border-emerald-500/30 bg-emerald-600 px-3 py-2 text-white shadow-lg">
            <CheckCircle2 className="mt-0.5 h-4 w-4 shrink-0" />
            <p className="text-sm leading-5">{skillsAutoUpdateNotice}</p>
            <button
              type="button"
              className="shrink-0 rounded-sm p-0.5 transition-colors hover:bg-white/20"
              aria-label={t("close", "Close")}
              onClick={() => setSkillsAutoUpdateNotice(null)}
            >
              <X className="h-4 w-4" />
            </button>
          </div>
        </div>
      )}

      {runtimeError && (
        <div className="fixed inset-x-4 bottom-4 z-[130]">
          <div className="mx-auto max-w-4xl rounded-xl border border-destructive/30 bg-card shadow-xl">
            <div className="flex items-start gap-3 p-4">
              <AlertCircle className="mt-0.5 h-5 w-5 shrink-0 text-destructive" />
              <div className="min-w-0 flex-1">
                <div className="text-sm font-semibold text-foreground">
                  {t("runtimeErrorTitle", "Runtime error")}
                </div>
                <pre className="mt-2 max-h-48 overflow-auto whitespace-pre-wrap break-words rounded-lg bg-muted p-3 text-xs text-destructive select-text">
                  {runtimeError}
                </pre>
                <div className="mt-3 flex items-center gap-2">
                  <button
                    type="button"
                    onClick={() => {
                      void copyRuntimeError();
                    }}
                    className="inline-flex items-center gap-2 rounded-md border px-3 py-1.5 text-sm hover:bg-muted"
                  >
                    {runtimeErrorCopied ? (
                      <Check className="h-4 w-4" />
                    ) : (
                      <Copy className="h-4 w-4" />
                    )}
                    {runtimeErrorCopied
                      ? t("stackCopied", "Copied")
                      : t("copyStack", "Copy stack")}
                  </button>
                </div>
              </div>
              <button
                type="button"
                className="shrink-0 rounded-sm p-0.5 transition-colors hover:bg-muted"
                aria-label={t(
                  "runtimeErrorCloseLabel",
                  "Close runtime error notice",
                )}
                onClick={() => {
                  setRuntimeError(null);
                  setRuntimeErrorCopied(false);
                }}
              >
                <X className="h-4 w-4" />
              </button>
            </div>
          </div>
        </div>
      )}

      <OmniSearch
        open={omniOpen}
        setOpen={setOmniOpen}
        onNavigate={(tab) => {
          navigateToTab(normalizeLegacyTabTarget(tab));
        }}
      />
      <MessageCenter
        open={messageCenterOpen}
        onOpenChange={setMessageCenterOpen}
        onNavigate={navigateToMessageTarget}
      />
      <AboutModal open={aboutOpen} onClose={() => setAboutOpen(false)} />
      <UpdateUpgradeModal
        open={updateDialogOpen}
        onClose={() => setUpdateDialogOpen(false)}
        currentVersion={updaterManifest?.currentVersion || "-"}
        latestVersion={updaterManifest?.version || "-"}
        releaseNotes={updaterManifest?.body || ""}
        status={updaterStatus}
        installable={updaterInstallable}
        downloadProgress={updaterDownloadProgress}
        onUpgradeNow={handleUpgradeNow}
        onIgnoreVersion={handleIgnoreVersion}
      />
    </div>
  );
}

export default App;
