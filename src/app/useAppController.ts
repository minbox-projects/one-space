import { useMemo } from "react";
import type { TFunction } from "i18next";
import { recordMessage } from "@/lib/messages";
import { useAppShellState } from "./useAppShellState";
import { useAppViewState } from "./useAppViewState";
import { useAppTrayState } from "./useAppTrayState";
import { useAppNoticeState } from "./useAppNoticeState";
import { useAppUpdater } from "./useAppUpdater";
import { useAppCounts } from "./useAppCounts";
import { useAppRuntimeSummary } from "./useAppRuntimeSummary";
import { useAppNavigation } from "./useAppNavigation";
import { useAppTrayController } from "./useAppTrayController";
import { useAppNotificationEffects } from "./useAppNotificationEffects";
import { useAppNavigationLifecycle } from "./useAppNavigationLifecycle";
import { useAppBootstrap } from "./useAppBootstrap";
import { useAppSchedulers } from "./useAppSchedulers";
import { useAppNavigationGroups } from "./useAppNavigationGroups";
import { useAppUpdaterEffects } from "./useAppUpdaterEffects";
import type { AppI18n } from "./types";

type Theme = "dark" | "light" | "system";

type PushToast = (toast: {
  title: string;
  description?: string;
  kind?: "info" | "success" | "warning" | "error" | "loading";
  durationMs?: number;
  action?: { label: string; onClick: () => void };
}) => string;

type UseAppControllerParams = {
  t: TFunction;
  i18n: AppI18n;
  theme: Theme;
  setTheme: (theme: Theme) => void;
  pushToast: PushToast;
  dismissToast: (id: string) => void;
};

export function useAppController({
  t,
  i18n,
  theme,
  setTheme,
  pushToast,
  dismissToast,
}: UseAppControllerParams) {
  const shell = useAppShellState();
  const view = useAppViewState();
  const tray = useAppTrayState();
  const notices = useAppNoticeState();

  const updater = useAppUpdater({
    t,
    ignoredUpdateVersion: notices.ignoredUpdateVersion,
    setIgnoredUpdateVersion: notices.setIgnoredUpdateVersion,
    setUpdateDialogOpen: notices.setUpdateDialogOpen,
    setAutoUpdateNoticeOpen: notices.setAutoUpdateNoticeOpen,
  });

  const counts = useAppCounts({
    setStorageType: view.setStorageType,
    setMessageUnreadCount: view.setMessageUnreadCount,
  });

  const queryParams = new URLSearchParams(window.location.search);
  const viewName = queryParams.get("view");
  const isQuickAiView = viewName === "quick-ai";
  const isTauri = counts.isTauri;
  const runtimeEnabled = isTauri && !isQuickAiView;

  const runtime = useAppRuntimeSummary({
    runtimeEnabled,
    gatewayBaseUrlRef: tray.gatewayBaseUrlRef,
    sshTunnelSummaryRef: tray.sshTunnelSummaryRef,
    setProtocolRouterHeaderStatus: view.setProtocolRouterHeaderStatus,
    setAiGatewayHeaderStatus: view.setAiGatewayHeaderStatus,
    setFileSharingHeaderStatus: view.setFileSharingHeaderStatus,
    setSshTunnelSummary: view.setSshTunnelSummary,
    setTrayState: tray.setTrayState,
    pushToast,
    recordMessage,
    t,
  });

  const nav = useAppNavigation({
    activeTab: shell.activeTab,
    activeTabRef: notices.activeTabRef,
    moreToolsSection: view.moreToolsSection,
    t,
    i18n,
    isTauri,
    setMobileNavigationOpen: shell.setMobileNavigationOpen,
    setMoreToolsSection: view.setMoreToolsSection,
    setMoreToolsReturnTab: view.setMoreToolsReturnTab,
    setJttParserTab: view.setJttParserTab,
    setSshTunnelTab: view.setSshTunnelTab,
    setSshTunnelNavigationNonce: view.setSshTunnelNavigationNonce,
    setPreviousTab: shell.setPreviousTab,
    setSettingsInitialTab: shell.setSettingsInitialTab,
    setActiveTab: shell.setActiveTab,
    setMessageCenterOpen: shell.setMessageCenterOpen,
  });

  useAppTrayController({
    isTauri,
    isQuickAiView,
    trayState: tray.trayState,
    setTrayState: tray.setTrayState,
    trayStateRef: tray.trayStateRef,
    trayMenuApplyRef: tray.trayMenuApplyRef,
    trayActionRef: tray.trayActionRef,
    gatewayBaseUrlRef: tray.gatewayBaseUrlRef,
    t,
    i18n,
    pushToast,
    navigateFromTray: nav.navigateFromTray,
    setAboutAutoCheck: shell.setAboutAutoCheck,
    setAboutOpen: shell.setAboutOpen,
  });

  const notificationEffects = useAppNotificationEffects({
    pushToast,
    t,
    setNetworkCircuitOpen: view.setNetworkCircuitOpen,
    ignoredUpdateVersion: notices.ignoredUpdateVersion,
    ignoredUpdateVersionRef: notices.ignoredUpdateVersionRef,
    skillsAutoUpdateNotice: notices.skillsAutoUpdateNotice,
    setSkillsAutoUpdateNotice: notices.setSkillsAutoUpdateNotice,
    runtimeError: notices.runtimeError,
    setRuntimeError: notices.setRuntimeError,
    setRuntimeErrorCopied: notices.setRuntimeErrorCopied,
  });

  const lifecycle = useAppNavigationLifecycle({
    activeTab: shell.activeTab,
    setMountedTabs: view.setMountedTabs,
    workspaceCapabilityNavigation: notices.workspaceCapabilityNavigation,
    setWorkspaceCapabilityNavigation: notices.setWorkspaceCapabilityNavigation,
    setActiveTab: shell.setActiveTab,
    previousTab: shell.previousTab,
    setPreviousTab: shell.setPreviousTab,
    setSettingsInitialTab: shell.setSettingsInitialTab,
    navigateToTab: nav.navigateToTab,
    isTauri,
    setOnboardingStatus: view.setOnboardingStatus,
    setMoreToolsReturnTab: view.setMoreToolsReturnTab,
    moreToolsReturnTab: view.moreToolsReturnTab,
    setMoreToolsSection: view.setMoreToolsSection,
    setJttParserTab: view.setJttParserTab,
  });

  useAppBootstrap({
    onboardingStatus: view.onboardingStatus,
    isQuickAiView,
    isTauri,
    scheduleLoadCounts: counts.scheduleLoadCounts,
    loadCounts: counts.loadCounts,
    refreshMessageUnreadCount: counts.refreshMessageUnreadCount,
    loadUsageTodayTokens: counts.loadUsageTodayTokens,
    countsRefreshTimerRef: counts.countsRefreshTimerRef,
    setCounts: counts.setCounts,
    setMessageUnreadCount: view.setMessageUnreadCount,
    pushToast,
    dismissToast,
    t,
    i18n,
    setPreviousTab: shell.setPreviousTab,
    setMoreToolsReturnTab: view.setMoreToolsReturnTab,
    setMoreToolsSection: view.setMoreToolsSection,
    setActiveTab: shell.setActiveTab,
    activeTabRef: notices.activeTabRef,
    setSyncStatus: view.setSyncStatus,
    setSyncError: view.setSyncError,
    setStorageType: view.setStorageType,
    setIgnoredUpdateVersion: notices.setIgnoredUpdateVersion,
    sshReconnectLoadingToastRef: tray.sshReconnectLoadingToastRef,
  });

  useAppSchedulers({
    isTauri,
    onboardingStatus: view.onboardingStatus,
    checkForUpdates: updater.checkForUpdates,
    downloadUpdateIfAvailable: updater.downloadUpdateIfAvailable,
    ignoredUpdateVersionRef: notices.ignoredUpdateVersionRef,
    t,
    pushToast,
    setSkillsAutoUpdateNotice: notices.setSkillsAutoUpdateNotice,
  });

  const navGroups = useAppNavigationGroups({
    counts: counts.counts,
    i18n,
    moreToolsLabel: nav.moreToolsLabel,
    t,
    aiGatewayHeaderStatus: view.aiGatewayHeaderStatus,
    usageTodayTokens: counts.usageTodayTokens,
    activeTab: shell.activeTab,
  });

  useAppUpdaterEffects({
    isTauri,
    onboardingStatus: view.onboardingStatus,
    showUpdateIndicator: updater.showUpdateIndicator,
    updaterError: updater.updaterError,
    updaterManifest: updater.updaterManifest,
    updaterStatus: updater.updaterStatus,
    pushToast,
    t,
    downloadUpdateIfAvailable: updater.downloadUpdateIfAvailable,
    updateDialogOpen: notices.updateDialogOpen,
    autoUpdateNoticeOpen: notices.autoUpdateNoticeOpen,
    setUpdateDialogOpen: notices.setUpdateDialogOpen,
    setAutoUpdateNoticeOpen: notices.setAutoUpdateNoticeOpen,
    updateMessageKeyRef: updater.updateMessageKeyRef,
  });

  const resolvedTheme = useMemo(
    () =>
      theme === "system"
        ? window.matchMedia("(prefers-color-scheme: dark)").matches
          ? "dark"
          : "light"
        : theme,
    [theme],
  );

  return {
    t,
    i18n,
    theme,
    setTheme,
    pushToast,
    dismissToast,
    isQuickAiView,
    isTauri,
    runtimeEnabled,
    resolvedTheme,
    shell,
    view,
    tray,
    notices,
    updater,
    counts,
    runtime,
    nav,
    notificationEffects,
    lifecycle,
    navGroups,
  };
}

export type AppController = ReturnType<typeof useAppController>;
