import { useCallback, useEffect, useMemo } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { TFunction } from "i18next";
import { resolveNavigationTarget } from "@/lib/navigation";
import type { MessageTarget } from "@/lib/messages";
import type { JttParserTab, MoreToolsSection } from "@/lib/navigation";
import { getToolboxTool, resolveToolboxText } from "@/toolbox/registry";
import type { AppI18n, AppStorageConfig, AppWindowBindings } from "./types";

type UseAppNavigationParams = {
  activeTab: string;
  activeTabRef: { current: string };
  moreToolsSection: MoreToolsSection | null;
  t: TFunction;
  i18n: AppI18n;
  isTauri: boolean;
  setMobileNavigationOpen: (open: boolean) => void;
  setMoreToolsSection: (section: MoreToolsSection | null) => void;
  setMoreToolsReturnTab: (tab: "launcher" | "more-tools") => void;
  setJttParserTab: (tab: JttParserTab | null) => void;
  setSshTunnelTab: (tab: string | null) => void;
  setSshTunnelNavigationNonce: (updater: (prev: number) => number) => void;
  setPreviousTab: (tab: string) => void;
  setSettingsInitialTab: (tab: string) => void;
  setActiveTab: (tab: string) => void;
  setMessageCenterOpen: (open: boolean) => void;
};

export function useAppNavigation({
  activeTab,
  activeTabRef,
  moreToolsSection,
  t,
  i18n,
  isTauri,
  setMobileNavigationOpen,
  setMoreToolsSection,
  setMoreToolsReturnTab,
  setJttParserTab,
  setSshTunnelTab,
  setSshTunnelNavigationNonce,
  setPreviousTab,
  setSettingsInitialTab,
  setActiveTab,
  setMessageCenterOpen,
}: UseAppNavigationParams) {
  const moreToolsLabel =
    i18n.language === "zh" ? "更多工具" : "More Tools";
  const moreToolsSectionTitle = useMemo(() => {
    if (!moreToolsSection) return null;
    const descriptor = getToolboxTool(moreToolsSection);
    return descriptor
      ? resolveToolboxText(descriptor.labelText, descriptor.labelKey, t)
      : null;
  }, [moreToolsSection, t]);

  // The setState functions and activeTabRef are stable for the lifetime of the
  // hook, so the original empty dependency list keeps the callback identity.
  // eslint-disable-next-line react-hooks/preserve-manual-memoization
  const navigateToTab = useCallback((target: string) => {
    setMobileNavigationOpen(false);
    const resolved = resolveNavigationTarget(target);

    if (resolved.moreToolsSection) {
      setMoreToolsSection(resolved.moreToolsSection);
      setMoreToolsReturnTab(
        activeTabRef.current === "launcher" ? "launcher" : "more-tools",
      );
    } else if (resolved.tab === "more-tools") {
      setMoreToolsSection(null);
    }
    setJttParserTab(resolved.jttParserTab ?? null);
    setSshTunnelTab(resolved.sshTunnelTab ?? null);
    if (resolved.sshTunnelTab) {
      setSshTunnelNavigationNonce((prev) => prev + 1);
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
  }, []);

  const navigateFromTray = useCallback(
    (target: string) => {
      const binding = (window as AppWindowBindings).setActiveTab;
      if (binding) {
        binding(target);
      } else {
        navigateToTab(target);
      }
    },
    [navigateToTab],
  );

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
    if (target.tab === "more-tools") {
      const currentTab = activeTabRef.current;
      if (currentTab !== "more-tools") {
        setPreviousTab(currentTab);
      }
      setMoreToolsReturnTab(currentTab === "launcher" ? "launcher" : "more-tools");
      setMoreToolsSection((target.section as MoreToolsSection) || null);
      setActiveTab("more-tools");
      return;
    }
    navigateToTab(target.tab);
  };

  useEffect(() => {
    activeTabRef.current = activeTab;
  }, [activeTab]);

  const toggleLanguage = async () => {
    const newLang = i18n.language === "zh" ? "en" : "zh";
    await i18n.changeLanguage(newLang);

    if (isTauri) {
      try {
        const cfg = await invoke<AppStorageConfig>("get_storage_config");
        await invoke("save_storage_config", {
          config: { ...cfg, language: newLang },
        });
      } catch (e) {
        console.error("Failed to save language preference:", e);
      }
    }
  };

  return {
    moreToolsLabel,
    moreToolsSectionTitle,
    navigateToTab,
    navigateFromTray,
    navigateToMessageTarget,
    toggleLanguage,
  };
}
