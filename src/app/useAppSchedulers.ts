import { useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { TFunction } from "i18next";
import { errorToMessage, recordMessage } from "@/lib/messages";
import { notifySystemEvent } from "@/lib/userActions";
import { getUpdaterState } from "@/lib/updater";
import type { ApiResp, AppStorageConfig, RepoAutoUpdateResult } from "./types";
import { SKILLS_AUTO_UPDATED_EVENT } from "./constants";

type UpdaterHook = {
  checkForUpdates: (
    silent?: boolean,
    manual?: boolean,
  ) => Promise<unknown>;
  downloadUpdateIfAvailable: (silent?: boolean) => Promise<unknown>;
};

type UseAppSchedulersParams = {
  isTauri: boolean;
  onboardingStatus: "checking" | "required" | "done";
  checkForUpdates: UpdaterHook["checkForUpdates"];
  downloadUpdateIfAvailable: UpdaterHook["downloadUpdateIfAvailable"];
  ignoredUpdateVersionRef: { current: string | null };
  t: TFunction;
  pushToast: (toast: {
    title: string;
    description?: string;
    kind?: "info" | "success" | "warning" | "error" | "loading";
  }) => string;
  setSkillsAutoUpdateNotice: (notice: string | null) => void;
};

export function useAppSchedulers({
  isTauri,
  onboardingStatus,
  checkForUpdates,
  downloadUpdateIfAvailable,
  ignoredUpdateVersionRef,
  t,
  pushToast,
  setSkillsAutoUpdateNotice,
}: UseAppSchedulersParams) {
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
}
