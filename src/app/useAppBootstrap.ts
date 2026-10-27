import { useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { TFunction } from "i18next";
import { getUnreadEmailCount } from "@/lib/gmail";
import { MESSAGES_UPDATED_EVENT, recordMessage } from "@/lib/messages";
import { notifySystemEvent } from "@/lib/userActions";
import {
  buildSshAutoConnectFailedEvent,
  buildSshWindowReconnectDoneEvent,
} from "@/lib/actionDescriptors/appSystemEvents";
import { USAGE_TODAY_TOKENS_REFRESH_MS } from "./constants";
import type { MoreToolsSection } from "@/lib/navigation";
import type { AppCounts, AppI18n, AppStorageConfig } from "./types";

type UseAppBootstrapParams = {
  onboardingStatus: "checking" | "required" | "done";
  isQuickAiView: boolean;
  isTauri: boolean;
  scheduleLoadCounts: (delayMs?: number) => void;
  loadCounts: () => Promise<void>;
  refreshMessageUnreadCount: () => Promise<void>;
  loadUsageTodayTokens: () => Promise<void>;
  countsRefreshTimerRef: { current: number | null };
  setCounts: (updater: AppCounts | ((prev: AppCounts) => AppCounts)) => void;
  setMessageUnreadCount: (count: number) => void;
  pushToast: (toast: {
    title: string;
    description?: string;
    kind?: "info" | "success" | "warning" | "error" | "loading";
    action?: { label: string; onClick: () => void };
  }) => string;
  dismissToast: (id: string) => void;
  t: TFunction;
  i18n: AppI18n;
  setPreviousTab: (tab: string) => void;
  setMoreToolsReturnTab: (tab: "launcher" | "more-tools") => void;
  setMoreToolsSection: (section: MoreToolsSection | null) => void;
  setActiveTab: (tab: string) => void;
  activeTabRef: { current: string };
  setSyncStatus: (
    status: "idle" | "pulling" | "pushing" | "success" | "error",
  ) => void;
  setSyncError: (error: string | null) => void;
  setStorageType: (storageType: "local" | "git" | "icloud") => void;
  setIgnoredUpdateVersion: (version: string | null) => void;
  sshReconnectLoadingToastRef: { current: string | null };
};

export function useAppBootstrap({
  onboardingStatus,
  isQuickAiView,
  isTauri,
  scheduleLoadCounts,
  loadCounts,
  refreshMessageUnreadCount,
  loadUsageTodayTokens,
  countsRefreshTimerRef,
  setCounts,
  setMessageUnreadCount,
  pushToast,
  dismissToast,
  t,
  i18n,
  setPreviousTab,
  setMoreToolsReturnTab,
  setMoreToolsSection,
  setActiveTab,
  activeTabRef,
  setSyncStatus,
  setSyncError,
  setStorageType,
  setIgnoredUpdateVersion,
  sshReconnectLoadingToastRef,
}: UseAppBootstrapParams) {
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

      addListener("uptime-monitor-alert", (event) => {
        const payload = (event.payload ?? {}) as {
          monitor_id?: string;
          name?: string;
          target?: string;
          status_kind?: "down" | "up";
          title?: string;
          body?: string;
        };
        const isDown = payload.status_kind === "down";
        void refreshMessageUnreadCount();

        pushToast({
          title:
            payload.title ||
            (isDown
              ? t("uptimeDownAlertTitle", "服务故障告警")
              : t("uptimeUpAlertTitle", "服务恢复通知")),
          description: payload.body || `${payload.name ?? ""} (${payload.target ?? ""})`,
          kind: isDown ? "error" : "success",
          action: {
            label: t("view", "查看"),
            onClick: () => {
              const currentTab = activeTabRef.current;
              if (currentTab !== "more-tools") {
                setPreviousTab(currentTab);
              }
              setMoreToolsReturnTab(currentTab === "launcher" ? "launcher" : "more-tools");
              setMoreToolsSection("uptime-monitor");
              setActiveTab("more-tools");
            },
          },
        });

        if (typeof window !== "undefined" && "Notification" in window) {
          if (Notification.permission === "granted") {
            try {
              new Notification(payload.title || "OneSpace 服务监控提醒", {
                body: payload.body || "",
              });
            } catch {
              // Ignore native notification errors
            }
          } else if (Notification.permission === "default") {
            Notification.requestPermission().then((perm) => {
              if (perm === "granted") {
                try {
                  new Notification(payload.title || "OneSpace 服务监控提醒", {
                    body: payload.body || "",
                  });
                } catch {}
              }
            });
          }
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

      addListener("refresh-mail-count", () => {
        getUnreadEmailCount()
          .then((mailCount) => {
            setCounts((prev) => ({ ...prev, mail: mailCount }));
          })
          .catch(() => {});
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

    let usageRefreshId: ReturnType<typeof setInterval> | null = null;
    if (isTauri) {
      void loadUsageTodayTokens();
      usageRefreshId = setInterval(() => {
        void loadUsageTodayTokens();
      }, USAGE_TODAY_TOKENS_REFRESH_MS);
    }

    return () => {
      if (timeoutId) clearTimeout(timeoutId);
      if (usageRefreshId) clearInterval(usageRefreshId);
      if (countsRefreshTimerRef.current !== null) {
        window.clearTimeout(countsRefreshTimerRef.current);
        countsRefreshTimerRef.current = null;
      }
      unlistenFns.forEach((fn) => fn());
    };
  }, [onboardingStatus, isQuickAiView]);
}
