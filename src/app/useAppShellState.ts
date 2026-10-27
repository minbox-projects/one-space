import { useCallback, useEffect, useState } from "react";

export function useAppShellState() {
  const [activeTab, setActiveTab] = useState("launcher");
  const [previousTab, setPreviousTab] = useState("launcher");
  const [fishPondPreviousTab, setFishPondPreviousTab] = useState("launcher");
  const [omniOpen, setOmniOpen] = useState(false);
  const [settingsInitialTab, setSettingsInitialTab] = useState("storage");
  const [aboutOpen, setAboutOpen] = useState(false);
  const [aboutAutoCheck, setAboutAutoCheck] = useState(false);
  const [messageCenterOpen, setMessageCenterOpen] = useState(false);
  const [mobileNavigationOpen, setMobileNavigationOpen] = useState(false);
  const [sidebarCollapsed, setSidebarCollapsed] = useState<boolean>(() => {
    try {
      return localStorage.getItem("onespace:sidebar-collapsed") === "true";
    } catch {
      return false;
    }
  });

  const toggleSidebar = useCallback(() => {
    setSidebarCollapsed((prev) => {
      const next = !prev;
      try {
        localStorage.setItem("onespace:sidebar-collapsed", String(next));
      } catch {}
      return next;
    });
  }, []);

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && (e.key === "b" || e.key === "B")) {
        // Don't trigger if user is typing in an input or textarea
        const target = e.target as HTMLElement | null;
        if (target && (target.tagName === "INPUT" || target.tagName === "TEXTAREA" || target.isContentEditable)) {
          return;
        }
        e.preventDefault();
        toggleSidebar();
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [toggleSidebar]);

  return {
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
    setAboutAutoCheck,
    messageCenterOpen,
    setMessageCenterOpen,
    mobileNavigationOpen,
    setMobileNavigationOpen,
    sidebarCollapsed,
    toggleSidebar,
  };
}
