import { useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { TFunction } from "i18next";
import type { UpdateManifest } from "@/lib/updater";
import { getUpdaterState } from "@/lib/updater";
import { buildUpdaterSystemEvent } from "@/lib/actionDescriptors/appSystemEvents";
import { recordMessage } from "@/lib/messages";
import { notifySystemEvent } from "@/lib/userActions";
import type { AppStorageConfig } from "./types";

type UpdaterStatus =
  | "idle"
  | "checking"
  | "available"
  | "downloading"
  | "downloaded"
  | "installing"
  | "error";

type UseAppUpdaterEffectsParams = {
  isTauri: boolean;
  onboardingStatus: "checking" | "required" | "done";
  showUpdateIndicator: boolean;
  updaterError: string | null;
  updaterManifest: UpdateManifest | null;
  updaterStatus: UpdaterStatus;
  pushToast: (toast: {
    title: string;
    description?: string;
    kind?: "info" | "success" | "warning" | "error" | "loading";
  }) => string;
  t: TFunction;
  downloadUpdateIfAvailable: (silent?: boolean) => Promise<unknown>;
  updateDialogOpen: boolean;
  autoUpdateNoticeOpen: boolean;
  setUpdateDialogOpen: (open: boolean) => void;
  setAutoUpdateNoticeOpen: (open: boolean) => void;
  updateMessageKeyRef: { current: string | null };
};

export function useAppUpdaterEffects({
  isTauri,
  onboardingStatus,
  showUpdateIndicator,
  updaterError,
  updaterManifest,
  updaterStatus,
  pushToast,
  t,
  downloadUpdateIfAvailable,
  updateDialogOpen,
  autoUpdateNoticeOpen,
  setUpdateDialogOpen,
  setAutoUpdateNoticeOpen,
  updateMessageKeyRef,
}: UseAppUpdaterEffectsParams) {
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
}
