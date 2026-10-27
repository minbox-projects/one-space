import { useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import type {
  CapabilityTargetTab,
  WorkspaceCapabilityContext,
} from "@/components/workspaceCapabilityContext";
import type { JttParserTab, MoreToolsSection } from "@/lib/navigation";
import type { AppWindowBindings } from "./types";

type UseAppNavigationLifecycleParams = {
  activeTab: string;
  setMountedTabs: (updater: (prev: Set<string>) => Set<string>) => void;
  workspaceCapabilityNavigation: {
    targetTab: CapabilityTargetTab;
    context: WorkspaceCapabilityContext;
  } | null;
  setWorkspaceCapabilityNavigation: (
    navigation:
      | { targetTab: CapabilityTargetTab; context: WorkspaceCapabilityContext }
      | null,
  ) => void;
  setActiveTab: (tab: string) => void;
  previousTab: string;
  setPreviousTab: (tab: string) => void;
  setSettingsInitialTab: (tab: string) => void;
  navigateToTab: (tab: string) => void;
  isTauri: boolean;
  setOnboardingStatus: (status: "checking" | "required" | "done") => void;
  setMoreToolsReturnTab: (tab: "launcher" | "more-tools") => void;
  moreToolsReturnTab: "launcher" | "more-tools";
  setMoreToolsSection: (section: MoreToolsSection | null) => void;
  setJttParserTab: (tab: JttParserTab | null) => void;
};

export function useAppNavigationLifecycle({
  activeTab,
  setMountedTabs,
  workspaceCapabilityNavigation,
  setWorkspaceCapabilityNavigation,
  setActiveTab,
  previousTab,
  setPreviousTab,
  setSettingsInitialTab,
  navigateToTab,
  isTauri,
  setOnboardingStatus,
  setMoreToolsReturnTab,
  moreToolsReturnTab,
  setMoreToolsSection,
  setJttParserTab,
}: UseAppNavigationLifecycleParams) {
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

  const handleSelectMoreTool = (tool: MoreToolsSection) => {
    setMoreToolsReturnTab("more-tools");
    setMoreToolsSection(tool);
    setJttParserTab(null);
  };

  const handleMoreToolsBack = () => {
    setMoreToolsSection(null);
    setJttParserTab(null);
    if (moreToolsReturnTab === "launcher") {
      setActiveTab("launcher");
    }
  };

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

  return {
    handleNavigateToCapability,
    clearWorkspaceCapabilityNavigation,
    handleSelectMoreTool,
    handleMoreToolsBack,
  };
}
