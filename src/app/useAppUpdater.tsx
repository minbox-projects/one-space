import { useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ArrowUpCircle, Loader2 } from "lucide-react";
import type { TFunction } from "i18next";
import { getUpdaterState, useUpdater } from "@/lib/updater";
import { openExternalUrl } from "@/lib/externalActions";
import type { AppStorageConfig } from "./types";

type UseAppUpdaterParams = {
  t: TFunction;
  ignoredUpdateVersion: string | null;
  setIgnoredUpdateVersion: (version: string | null) => void;
  setUpdateDialogOpen: (open: boolean) => void;
  setAutoUpdateNoticeOpen: (open: boolean) => void;
};

export function useAppUpdater({
  t,
  ignoredUpdateVersion,
  setIgnoredUpdateVersion,
  setUpdateDialogOpen,
  setAutoUpdateNoticeOpen,
}: UseAppUpdaterParams) {
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

  return {
    updaterStatus,
    updaterManifest,
    updaterInstallable,
    updaterDownloadProgress,
    checkForUpdates,
    downloadUpdateIfAvailable,
    installDownloadedUpdate,
    installUpdate,
    updaterError,
    updateMessageKeyRef,
    currentUpdateVersion,
    showUpdateIndicator,
    updateIndicatorTitle,
    openReleasesPage,
    handleUpgradeNow,
    handleIgnoreVersion,
    renderUpdateIndicatorIcon,
    setUpdateDialogOpen,
    setAutoUpdateNoticeOpen,
  };
}
