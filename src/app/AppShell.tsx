import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  Bell,
  Rocket,
  Newspaper,
  Search,
  Mail as MailIcon,
  Settings,
  Moon,
  Sun,
  Monitor,
  Info,
  Github,
  Fish,
  Loader2,
  Waypoints,
  CheckCircle2,
  AlertCircle,
  ArrowUpCircle,
  Check,
  Copy,
  Menu,
  X,
  Route,
  Globe,
  Network,
  Share2,
  PanelLeftClose,
  PanelLeftOpen,
} from "lucide-react";
import { Tooltip } from "@/components/ui/tooltip";
import { OmniSearch } from "@/components/OmniSearch";
import { AboutModal } from "@/components/AboutModal";
import { ToolStatusDot } from "@/components/toolbox/ToolStatusDot";
import { UpdateUpgradeModal } from "@/components/UpdateUpgradeModal";
import { MessageCenter } from "@/components/MessageCenter";
import { NETWORK_CIRCUIT_MESSAGE } from "@/lib/networkCircuitBreaker";
import { openExternalUrl } from "@/lib/externalActions";
import logoWhite from "@/assets/onespace_logo_white.png";
import logoBlack from "@/assets/onespace_logo_black.png";
import { TrayBleLockBridge } from "./TrayBleLockBridge";
import { AppTabContent } from "./AppTabContent";
import type { AppController } from "./useAppController";

export function AppShell(props: AppController) {
  const {
    t,
    i18n,
    theme,
    setTheme,
    isTauri,
    runtimeEnabled,
    resolvedTheme,
    shell,
    view,
    notices,
    updater,
    counts: countsHook,
    runtime,
    nav,
    notificationEffects,
    lifecycle,
    navGroups,
  } = props;

  const {
    activeTab,
    setActiveTab,
    previousTab,
    setPreviousTab,
    fishPondPreviousTab,
    setFishPondPreviousTab,
    omniOpen,
    setOmniOpen,
    settingsInitialTab,
    setSettingsInitialTab,
    aboutOpen,
    setAboutOpen,
    aboutAutoCheck,
    messageCenterOpen,
    setMessageCenterOpen,
    mobileNavigationOpen,
    setMobileNavigationOpen,
    sidebarCollapsed,
    toggleSidebar,
  } = shell;

  const {
    messageUnreadCount,
    moreToolsSection,
    jttParserTab,
    sshTunnelTab,
    sshTunnelNavigationNonce,
    moreToolsReturnTab,
    storageType,
    syncStatus,
    syncError,
    setSyncError,
    networkCircuitOpen,
    setNetworkCircuitOpen,
    sshTunnelSummary,
    protocolRouterHeaderStatus,
    aiGatewayHeaderStatus,
    fileSharingHeaderStatus,
    mountedTabs,
  } = view;

  const {
    skillsAutoUpdateNotice,
    setSkillsAutoUpdateNotice,
    runtimeError,
    setRuntimeError,
    runtimeErrorCopied,
    setRuntimeErrorCopied,
    workspaceCapabilityNavigation,
    updateDialogOpen,
    setUpdateDialogOpen,
    autoUpdateNoticeOpen,
    setAutoUpdateNoticeOpen,
  } = notices;

  const {
    updaterStatus,
    updaterManifest,
    updaterInstallable,
    updaterDownloadProgress,
    showUpdateIndicator,
    updateIndicatorTitle,
    handleUpgradeNow,
    handleIgnoreVersion,
    renderUpdateIndicatorIcon,
  } = updater;

  const { counts: dashboardCounts, scheduleLoadCounts } = countsHook;

  const {
    portTunnelSummary,
    handleTrayBleLockStatus,
  } = runtime;

  const {
    moreToolsLabel,
    moreToolsSectionTitle,
    navigateToTab,
    navigateToMessageTarget,
    toggleLanguage,
  } = nav;

  const {
    handleNavigateToCapability,
    clearWorkspaceCapabilityNavigation,
    handleSelectMoreTool,
    handleMoreToolsBack,
  } = lifecycle;

  const { navigationGroups, isNavigationItemActive } = navGroups;
  const { copyRuntimeError } = notificationEffects;

  const copySyncError = () => {
    if (syncError) {
      navigator.clipboard.writeText(syncError);
      const originalError = syncError;
      setSyncError(t("copied", "Copied!"));
      setTimeout(() => setSyncError(originalError), 2000);
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

  const ThemeIcon =
    theme === "system" ? Monitor : theme === "dark" ? Moon : Sun;
  const themeLabel =
    theme === "system"
      ? t("themeSystem")
      : theme === "dark"
        ? t("themeDark")
        : t("themeLight");

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

  return (
    <div className="flex h-screen bg-background text-foreground overflow-hidden select-none">
      <TrayBleLockBridge
        enabled={runtimeEnabled}
        onStatusChange={handleTrayBleLockStatus}
      />
      <div
        className={`${mobileNavigationOpen ? "flex" : "hidden"} fixed inset-y-0 left-0 z-50 ${
          sidebarCollapsed ? "w-16" : "w-64"
        } shrink-0 border-r bg-muted/20 flex-col md:static md:z-auto md:flex transition-all duration-200 ease-in-out`}
      >
        <div
          className={`h-16 flex items-end ${
            sidebarCollapsed ? "justify-center px-2" : "justify-between pl-5 pr-3"
          } pb-2 border-b font-semibold tracking-tight cursor-default select-none relative`}
          data-tauri-drag-region
          onMouseDown={handleDragMouseDown}
        >
          {!sidebarCollapsed ? (
            <>
              <div className="flex items-center gap-2 pointer-events-none">
                <img
                  src={resolvedTheme === "dark" ? logoWhite : logoBlack}
                  alt="OneSpace"
                  className="w-5 h-5"
                />
                <span className="text-lg">OneSpace</span>
                {import.meta.env.DEV ? (
                  <span
                    data-testid="app-dev-badge"
                    title="Development build"
                    aria-label="Development build"
                    className="rounded-md border border-amber-500/40 bg-amber-500/15 px-1.5 py-0.5 text-[10px] font-bold tracking-widest text-amber-600 dark:text-amber-400"
                  >
                    DEV
                  </span>
                ) : null}
              </div>
              <div className="flex items-center gap-1">
                <button
                  type="button"
                  onClick={toggleSidebar}
                  className="hidden md:flex h-7 w-7 items-center justify-center rounded-md text-muted-foreground hover:bg-muted hover:text-foreground transition-colors"
                  aria-label="Collapse sidebar (⌘B)"
                  title="Collapse sidebar (⌘B)"
                >
                  <PanelLeftClose className="h-4 w-4" />
                </button>
                <button
                  type="button"
                  onClick={() => setMobileNavigationOpen(false)}
                  className="flex h-8 w-8 items-center justify-center rounded-md hover:bg-muted md:hidden"
                  aria-label={t("close", "Close")}
                  title={t("close", "Close")}
                >
                  <X className="h-4 w-4" />
                </button>
              </div>
            </>
          ) : (
            <Tooltip content="Expand sidebar (⌘B)" side="right">
              <button
                type="button"
                onClick={toggleSidebar}
                className="h-8 w-8 flex items-center justify-center rounded-md hover:bg-muted text-muted-foreground hover:text-foreground transition-colors"
                aria-label="Expand sidebar (⌘B)"
                title="Expand sidebar (⌘B)"
              >
                <PanelLeftOpen className="h-4 w-4" />
              </button>
            </Tooltip>
          )}
        </div>

        <div
          className={`flex-1 overflow-y-auto py-4 ${
            sidebarCollapsed ? "px-2 space-y-4" : "px-3 space-y-5"
          }`}
        >
          {navigationGroups.map((group, groupIdx) => (
            <div key={group.id} className="space-y-1.5">
              {!sidebarCollapsed ? (
                <div className="px-3 text-[11px] font-semibold uppercase tracking-[0.18em] text-muted-foreground">
                  {group.label}
                </div>
              ) : (
                groupIdx > 0 && <div className="my-2 border-t border-border/40 mx-1" />
              )}
              {group.items.map((item: any) => {
                const selected = isNavigationItemActive(item.id);
                if (sidebarCollapsed) {
                  return (
                    <Tooltip key={item.id} content={item.name} side="right">
                      <button
                        onClick={() => navigateToTab(item.id)}
                        aria-label={item.name}
                        className={`w-10 h-10 mx-auto flex items-center justify-center rounded-md text-sm transition-colors relative ${
                          selected
                            ? "bg-primary text-primary-foreground font-medium shadow-sm"
                            : "hover:bg-muted text-muted-foreground hover:text-foreground"
                        }`}
                      >
                        <item.icon className="w-4 h-4" />
                        {item.count !== undefined && item.count > 0 && (
                          <span className="absolute top-1.5 right-1.5 w-2 h-2 rounded-full bg-primary ring-2 ring-background" />
                        )}
                      </button>
                    </Tooltip>
                  );
                }
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
                    </div>
                    {item.count !== undefined && (
                      <span
                        className={`text-[10px] px-1.5 py-0.5 rounded-full font-mono ${
                          selected
                            ? "bg-primary-foreground/20 text-primary-foreground"
                            : "bg-muted-foreground/10 text-muted-foreground"
                        }`}
                      >
                        {item.countText ?? item.count}
                      </span>
                    )}
                  </button>
                );
              })}
            </div>
          ))}
        </div>

        <div
          className={`border-t ${
            sidebarCollapsed
              ? "p-2 space-y-2 flex flex-col items-center"
              : "p-3 space-y-1"
          }`}
        >
          {!sidebarCollapsed ? (
            <>
              <button
                onClick={() => {
                  setMobileNavigationOpen(false);
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
                onClick={() => {
                  setMobileNavigationOpen(false);
                  setAboutOpen(true);
                }}
                className="w-full flex items-center gap-3 px-3 py-2 rounded-md text-sm text-muted-foreground hover:bg-muted hover:text-foreground transition-colors"
              >
                <Info className="w-4 h-4" />
                {t("about")}
              </button>
            </>
          ) : (
            <>
              <Tooltip content={t("settings")} side="right">
                <button
                  onClick={() => {
                    setMobileNavigationOpen(false);
                    setPreviousTab(activeTab);
                    setActiveTab("settings");
                  }}
                  aria-label={t("settings")}
                  className={`w-10 h-10 flex items-center justify-center rounded-md text-sm transition-colors ${
                    activeTab === "settings"
                      ? "bg-primary/10 text-primary font-medium"
                      : "text-muted-foreground hover:bg-muted hover:text-foreground"
                  }`}
                >
                  <Settings className="w-4 h-4" />
                </button>
              </Tooltip>
              <Tooltip content={t("about")} side="right">
                <button
                  onClick={() => {
                    setMobileNavigationOpen(false);
                    setAboutOpen(true);
                  }}
                  aria-label={t("about")}
                  className="w-10 h-10 flex items-center justify-center rounded-md text-sm text-muted-foreground hover:bg-muted hover:text-foreground transition-colors"
                >
                  <Info className="w-4 h-4" />
                </button>
              </Tooltip>
            </>
          )}
        </div>
      </div>

      <div className="flex-1 min-w-0 flex flex-col overflow-hidden relative bg-background">
        {activeTab !== "settings" && (
          <header
            className="h-16 border-b flex items-end px-3 pb-1.5 justify-between bg-background/95 backdrop-blur sm:px-6 supports-[backdrop-filter]:bg-background/60 relative"
            data-tauri-drag-region
            onMouseDown={handleDragMouseDown}
          >
            <div className="flex-1 flex items-center gap-4">
              <button
                type="button"
                onClick={() => setMobileNavigationOpen(true)}
                className="flex h-9 w-9 shrink-0 items-center justify-center rounded-md hover:bg-muted md:hidden"
                aria-label={t("navigation", "Navigation")}
                title={t("navigation", "Navigation")}
              >
                <Menu className="h-4 w-4" />
              </button>
              {activeTab === "more-tools" && moreToolsSectionTitle ? (
                <nav
                  aria-label={t("breadcrumb", "Breadcrumb")}
                  className="hidden min-w-0 items-center gap-2 text-sm sm:flex shrink-0"
                >
                  <span className="hidden text-muted-foreground sm:inline">
                    {moreToolsLabel}
                  </span>
                  <span
                    aria-hidden="true"
                    className="hidden text-muted-foreground/60 sm:inline"
                  >
                    /
                  </span>
                  <h1 className="truncate text-sm font-semibold" aria-current="page">
                    {moreToolsSectionTitle}
                  </h1>
                </nav>
              ) : null}

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

            <div className="hidden items-center gap-1 sm:flex">
              {/* 核心后台状态平铺 */}
              {aiGatewayHeaderStatus?.running && (
                <button
                  onClick={() => navigateToTab("ai-gateway")}
                  className="relative p-2.5 rounded-md transition-colors text-emerald-600 hover:bg-emerald-500/10 dark:text-emerald-400"
                  title={t("launcherAiGatewayRunningAria", {
                    port: aiGatewayHeaderStatus.port,
                    defaultValue: `AI Gateway running on port ${aiGatewayHeaderStatus.port}`,
                  })}
                  aria-label={t("launcherAiGatewayRunningAria", {
                    port: aiGatewayHeaderStatus.port,
                    defaultValue: `AI Gateway running on port ${aiGatewayHeaderStatus.port}`,
                  })}
                  data-testid="header-ai-gateway-status"
                >
                  <Network className="w-5 h-5" />
                  <ToolStatusDot tone="success" />
                </button>
              )}
              {protocolRouterHeaderStatus?.enabled && (
                <button
                  onClick={() => navigateToTab("protocol-router")}
                  className={`relative p-2.5 rounded-md transition-colors ${
                    protocolRouterHeaderStatus.running
                      ? "text-emerald-600 hover:bg-emerald-500/10 dark:text-emerald-400"
                      : "text-amber-600 hover:bg-amber-500/10 dark:text-amber-400"
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
                  data-testid="header-protocol-router-status"
                >
                  <Route className="w-5 h-5" />
                  <ToolStatusDot
                    tone={protocolRouterHeaderStatus.running ? "success" : "warning"}
                  />
                </button>
              )}
              {sshTunnelSummary && sshTunnelSummary.connectedCount > 0 && (
                <button
                  onClick={() => navigateToTab("ssh-tunnels:connected")}
                  className={`relative p-2.5 rounded-md transition-colors ${
                    sshTunnelSummary.hasErrors
                      ? "text-destructive hover:bg-destructive/10 dark:text-destructive"
                      : sshTunnelSummary.hasConnecting
                        ? "text-amber-600 hover:bg-amber-500/10 dark:text-amber-400"
                        : "text-emerald-600 hover:bg-emerald-500/10 dark:text-emerald-400"
                  }`}
                  title={
                    sshTunnelSummary.hasErrors
                      ? `${sshTunnelSummary.connectedCount} connected, ${sshTunnelSummary.errorTunnelNames.join(", ")} disconnected`
                      : sshTunnelSummary.hasConnecting
                        ? `${sshTunnelSummary.connectedCount} connected, connecting...`
                        : `${sshTunnelSummary.connectedCount} SSH tunnel${sshTunnelSummary.connectedCount > 1 ? "s" : ""} connected`
                  }
                  aria-label={
                    sshTunnelSummary.hasErrors
                      ? t("launcherSshTunnelsErrorAria", {
                          count: sshTunnelSummary.connectedCount,
                          errors: sshTunnelSummary.errorTunnelNames.join(", "),
                          defaultValue: `${sshTunnelSummary.connectedCount} connected, ${sshTunnelSummary.errorTunnelNames.join(", ")} disconnected`,
                        })
                      : sshTunnelSummary.hasConnecting
                        ? t("launcherSshTunnelsConnectingAria", {
                            count: sshTunnelSummary.connectedCount,
                            defaultValue: `${sshTunnelSummary.connectedCount} connected, connecting...`,
                          })
                        : t("launcherSshTunnelsRunningAria", {
                            count: sshTunnelSummary.connectedCount,
                            defaultValue: `${sshTunnelSummary.connectedCount} SSH tunnel(s) connected`,
                          })
                  }
                  data-testid="header-ssh-tunnels-status"
                >
                  <Waypoints className="w-5 h-5" />
                  <ToolStatusDot
                    tone={
                      sshTunnelSummary.hasErrors
                        ? "error"
                        : sshTunnelSummary.hasConnecting
                          ? "warning"
                          : "success"
                    }
                  />
                </button>
              )}
              {fileSharingHeaderStatus?.running && (
                <button
                  onClick={() => navigateToTab("file-sharing")}
                  className="relative p-2.5 rounded-md transition-colors text-emerald-600 hover:bg-emerald-500/10 dark:text-emerald-400"
                  title={t("launcherFileSharingRunningAria", {
                    files: fileSharingHeaderStatus.fileCount,
                    defaultValue: `File sharing running with ${fileSharingHeaderStatus.fileCount} shared file(s)`,
                  })}
                  aria-label={t("launcherFileSharingRunningAria", {
                    files: fileSharingHeaderStatus.fileCount,
                    defaultValue: `File sharing running with ${fileSharingHeaderStatus.fileCount} shared file(s)`,
                  })}
                  data-testid="header-file-sharing-status"
                >
                  <Share2 className="w-5 h-5" />
                  <ToolStatusDot tone="success" />
                </button>
              )}
              {portTunnelSummary && portTunnelSummary.activeCount > 0 && (
                <button
                  onClick={() => navigateToTab("port-tunnel")}
                  className={`relative p-2.5 rounded-md transition-colors ${
                    portTunnelSummary.hasErrors
                      ? "text-destructive hover:bg-destructive/10 dark:text-destructive"
                      : portTunnelSummary.hasStarting
                        ? "text-amber-600 hover:bg-amber-500/10 dark:text-amber-400"
                        : "text-emerald-600 hover:bg-emerald-500/10 dark:text-emerald-400"
                  }`}
                  title={
                    portTunnelSummary.hasErrors
                      ? t("launcherPortTunnelErrorAria", {
                          count: portTunnelSummary.connectedCount,
                          defaultValue: `${portTunnelSummary.connectedCount} connected, with errors`,
                        })
                      : portTunnelSummary.hasStarting
                        ? t("launcherPortTunnelConnectingAria", {
                            count: portTunnelSummary.connectedCount,
                            defaultValue: `${portTunnelSummary.connectedCount} connected, connecting...`,
                          })
                        : t("launcherPortTunnelRunningAria", {
                            count: portTunnelSummary.connectedCount,
                            defaultValue: `${portTunnelSummary.connectedCount} port tunnel${portTunnelSummary.connectedCount > 1 ? "s" : ""} connected`,
                          })
                  }
                  aria-label={
                    portTunnelSummary.hasErrors
                      ? t("launcherPortTunnelErrorAria", {
                          count: portTunnelSummary.connectedCount,
                          defaultValue: `${portTunnelSummary.connectedCount} connected, with errors`,
                        })
                      : portTunnelSummary.hasStarting
                        ? t("launcherPortTunnelConnectingAria", {
                            count: portTunnelSummary.connectedCount,
                            defaultValue: `${portTunnelSummary.connectedCount} connected, connecting...`,
                          })
                        : t("launcherPortTunnelRunningAria", {
                            count: portTunnelSummary.connectedCount,
                            defaultValue: `${portTunnelSummary.connectedCount} port tunnel(s) connected`,
                          })
                  }
                  data-testid="header-port-tunnel-status"
                >
                  <Globe className="w-5 h-5" />
                  <ToolStatusDot
                    tone={
                      portTunnelSummary.hasErrors
                        ? "error"
                        : portTunnelSummary.hasStarting
                          ? "warning"
                          : "success"
                    }
                  />
                </button>
              )}

              {/* 细微发丝分割线 */}
              <div className="h-4 w-[1px] bg-border/60 mx-1" />

              {/* 工作通知与消息 */}
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
                {dashboardCounts.mail > 0 ? (
                  <span className="absolute -right-0.5 -top-0.5 min-w-5 rounded-full bg-primary px-1.5 py-0.5 text-[10px] font-semibold text-primary-foreground">
                    {dashboardCounts.mail > 99 ? "99+" : dashboardCounts.mail}
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
                {dashboardCounts.aiNews > 0 ? (
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

              {/* 细微发丝分割线 */}
              <div className="h-4 w-[1px] bg-border/60 mx-1" />

              {/* 全局控制 */}
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
          <AppTabContent
            activeTab={activeTab}
            mountedTabs={mountedTabs}
            navigateToTab={navigateToTab}
            handleNavigateToCapability={handleNavigateToCapability}
            workspaceCapabilityNavigation={workspaceCapabilityNavigation}
            clearWorkspaceCapabilityNavigation={
              clearWorkspaceCapabilityNavigation
            }
            handleSelectMoreTool={handleSelectMoreTool}
            handleMoreToolsBack={handleMoreToolsBack}
            moreToolsSection={moreToolsSection}
            moreToolsReturnTab={moreToolsReturnTab}
            jttParserTab={jttParserTab}
            sshTunnelTab={sshTunnelTab}
            sshTunnelNavigationNonce={sshTunnelNavigationNonce}
            settingsInitialTab={settingsInitialTab}
            previousTab={previousTab}
            setActiveTab={setActiveTab}
            setSettingsInitialTab={setSettingsInitialTab}
            scheduleLoadCounts={scheduleLoadCounts}
          />
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
                      onClick={() => void updater.installDownloadedUpdate()}
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
          navigateToTab(tab);
        }}
      />
      <MessageCenter
        open={messageCenterOpen}
        onOpenChange={setMessageCenterOpen}
        onNavigate={navigateToMessageTarget}
      />
      <AboutModal
        open={aboutOpen}
        onClose={() => setAboutOpen(false)}
        autoCheckOnOpen={aboutAutoCheck}
        onOpenDocs={() => {
          setAboutOpen(false);
          navigateToTab("documentation");
        }}
      />
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
