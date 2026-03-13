import { useState, useEffect, useMemo, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { open } from '@tauri-apps/plugin-shell';
import { useTranslation } from 'react-i18next';
import { useTheme } from './components/ThemeProvider';
import { 
   Rocket, 
   Terminal, 
   Server, 
   Code2, 
   Star, 
   Sparkles,
   Newspaper,
   StickyNote, 
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
   CheckCircle2,
   AlertCircle,
   ArrowUpCircle
} from 'lucide-react';
import { AiSessions } from './components/AiSessions';
import { AiEnvironments } from './components/AiEnvironments';
import { Skills } from './components/Skills';
import { Subagents } from './components/Subagents';
import { MCPServers } from './components/MCPServers';
import { SshServers } from './components/SshServers';
import { Snippets } from './components/Snippets';
import { Bookmarks } from './components/Bookmarks';
import { Notes } from './components/Notes';
import { CloudDrive } from './components/CloudDrive';
import { Mail } from './components/Mail';
import { AiNews } from './components/AiNews';
import { OmniSearch } from './components/OmniSearch';
import { Launcher } from './components/Launcher';
import { SettingsView } from './components/SettingsView';
import { AboutModal } from './components/AboutModal';
import { QuickAiSessionBar } from './components/QuickAiSessionBar';
import { Documentation } from './components/Documentation';
import { OnboardingWizard } from './components/OnboardingWizard';
import { FishPond } from './components/FishPond';
import { UpdateUpgradeModal } from './components/UpdateUpgradeModal';
import { getUpdaterState, useUpdater } from './lib/updater';

import { getUnreadEmailCount } from './lib/gmail';
import logoWhite from './assets/onespace_logo_white.png';
import logoBlack from './assets/onespace_logo_black.png';

type ApiResp<T> = { ok: boolean; data: T; meta: { schema_version: number; revision: number } };
type TrayActionPayload = { action?: string; target?: string };

type DashboardCounts = {
  launcher: number;
  sessions: number;
  ssh: number;
  snippets: number;
  bookmarks: number;
  notes: number;
  environments: number;
  skills: number;
  subagents: number;
  mcp_servers: number;
  storage_type?: 'local' | 'git' | 'icloud';
};

const TRAY_NAV_TABS = new Set([
  'launcher',
  'ai-sessions',
  'ai-environments',
  'ai-news',
  'skills',
  'subagents',
  'mcp-servers',
  'ssh',
  'snippets',
  'bookmarks',
  'notes',
  'cloud',
  'mail',
  'documentation',
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

function App() {
  const { t, i18n } = useTranslation();
  const { theme, setTheme } = useTheme();
  
  // URL View Routing
  const queryParams = new URLSearchParams(window.location.search);
  const view = queryParams.get('view');
  const isQuickAiView = view === 'quick-ai';

  const [activeTab, setActiveTab] = useState('launcher');
  const [previousTab, setPreviousTab] = useState('launcher');
  const [fishPondPreviousTab, setFishPondPreviousTab] = useState('launcher');
  const [omniOpen, setOmniOpen] = useState(false);
  const [settingsInitialTab, setSettingsInitialTab] = useState('storage');
  const [aboutOpen, setAboutOpen] = useState(false);
  const [storageType, setStorageType] = useState<'local' | 'git' | 'icloud'>('local');
  const [onboardingStatus, setOnboardingStatus] = useState<'checking' | 'required' | 'done'>('checking');
  const [mountedTabs, setMountedTabs] = useState<Set<string>>(() => new Set(['launcher']));

  // Git Sync Status
  const [syncStatus, setSyncStatus] = useState<'idle' | 'pulling' | 'pushing' | 'success' | 'error'>('idle');
  const [syncError, setSyncError] = useState<string | null>(null);
  const [updateDialogOpen, setUpdateDialogOpen] = useState(false);
  const [ignoredUpdateVersion, setIgnoredUpdateVersion] = useState<string | null>(null);
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
  } = useUpdater();

  // Global counts for sidebar
  const [counts, setCounts] = useState({
    launcher: 0,
    sessions: 0,
    ssh: 0,
    snippets: 0,
    bookmarks: 0,
    notes: 0,
    mail: 0,
    environments: 0,
    skills: 0,
    subagents: 0,
    mcpServers: 0,
  });
  const loadCountsInFlightRef = useRef<Promise<void> | null>(null);
  const countsRefreshTimerRef = useRef<number | null>(null);

  const isTauri = '__TAURI_INTERNALS__' in window;
  useEffect(() => {
    activeTabRef.current = activeTab;
  }, [activeTab]);
  useEffect(() => {
    ignoredUpdateVersionRef.current = ignoredUpdateVersion;
  }, [ignoredUpdateVersion]);

  const handleDragMouseDown = (e: React.MouseEvent<HTMLElement>) => {
    const target = e.target as HTMLElement;
    if (target.closest('button,input,select,textarea,a,[role="button"],[data-no-drag]')) {
      return;
    }
    if (!isTauri) return;
    getCurrentWindow().startDragging().catch(() => {});
  };

  const loadCounts = async () => {
    if (!isTauri) return;

    if (loadCountsInFlightRef.current) {
      await loadCountsInFlightRef.current;
      return;
    }

    const task = (async () => {
      try {
        const res = await invoke<ApiResp<DashboardCounts>>('dashboard_counts');
        const data = res.data;
        setCounts(prev => ({
          ...prev,
          launcher: data.launcher || 0,
          sessions: data.sessions || 0,
          ssh: data.ssh || 0,
          snippets: data.snippets || 0,
          bookmarks: data.bookmarks || 0,
          notes: data.notes || 0,
          environments: data.environments || 0,
          skills: data.skills || 0,
          subagents: data.subagents || 0,
          mcpServers: data.mcp_servers || 0,
        }));
        if (data.storage_type) {
          setStorageType(data.storage_type);
        }
      } catch (e) {
        console.error('Failed to load local counts', e);
      }

      getUnreadEmailCount()
        .then(mailCount => {
          setCounts(prev => ({ ...prev, mail: mailCount }));
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

  useEffect(() => {
    setMountedTabs(prev => {
      if (prev.has(activeTab)) return prev;
      const next = new Set(prev);
      next.add(activeTab);
      return next;
    });
  }, [activeTab]);

  // Expose global navigation for components
  useEffect(() => {
    (window as any).setActiveTab = setActiveTab;
    (window as any).setSettingsOpen = (open: boolean) => {
      if (open) {
        setPreviousTab(activeTab);
        setActiveTab('settings');
      } else {
        setActiveTab(previousTab);
      }
    };
    (window as any).setSettingsTab = setSettingsInitialTab;
  }, [activeTab, previousTab]);

  useEffect(() => {
    if (!isTauri) {
      setOnboardingStatus('done');
      return;
    }
    invoke<boolean>('should_show_onboarding')
      .then((shouldShow) => {
        setOnboardingStatus(shouldShow ? 'required' : 'done');
      })
      .catch(() => setOnboardingStatus('done'));
  }, []);

  // Initial load and poll
  useEffect(() => {
    if (isQuickAiView) {
      return;
    }
    if (onboardingStatus !== 'done') {
      return;
    }

    const unlistenFns: Array<() => void> = [];
    const addListener = (
      eventName: string,
      handler: (event: { payload?: unknown }) => void
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
      invoke('show_main_window').catch(console.error);
      setTimeout(() => {
        scheduleLoadCounts(0);
      }, 500);

      setTimeout(() => {
        invoke('sync_run_now').catch(e => console.error("Sync failed:", e));
      }, 3000);

      addListener('trigger-sync', () => {
        invoke('sync_run_now').catch(e => console.error("Tray Sync failed:", e));
      });

      addListener('refresh-counts', () => {
        scheduleLoadCounts();
      });

      addListener('refresh-mail-count', () => {
        getUnreadEmailCount()
          .then(mailCount => {
            setCounts(prev => ({ ...prev, mail: mailCount }));
          })
          .catch(() => {});
      });

      addListener('tray-action', (event) => {
        const payload = (event.payload ?? {}) as TrayActionPayload;
        if (payload.action !== 'navigate' || !payload.target) {
          return;
        }
        if (payload.target === 'omni-search') {
          setOmniOpen(true);
          return;
        }
        if (payload.target === 'settings') {
          const current = activeTabRef.current;
          if (current !== 'settings') {
            setPreviousTab(current);
          }
          setSettingsInitialTab('storage');
          setActiveTab('settings');
          return;
        }
        if (TRAY_NAV_TABS.has(payload.target)) {
          setActiveTab(payload.target);
        }
      });

      addListener('git-sync-status', (event) => {
        const payload = (event.payload ?? {}) as { status?: string; message?: string };
        const status = payload.status as 'pulling' | 'pushing' | 'success' | 'error';
        if (!status) return;
        setSyncStatus(status);
        if (status === 'error') {
          setSyncError(payload.message || 'Unknown sync error');
        } else {
          setSyncError(null);
        }

        if (status === 'success') {
          scheduleLoadCounts(120);
          setTimeout(() => setSyncStatus('idle'), 3000);
        }
      });

      invoke<any>('get_storage_config').then(cfg => {
        if (cfg.language) {
          i18n.changeLanguage(cfg.language);
        }
        if (cfg.storage_type) {
          setStorageType(cfg.storage_type);
        }
        setIgnoredUpdateVersion(cfg.update_ignored_version ?? null);
      }).catch(e => console.error("Failed to load language", e));
    }
    
    let timeoutId: any;
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
    if (!isTauri || onboardingStatus !== 'done') {
      return;
    }

    let initialTimer: ReturnType<typeof setTimeout> | null = null;
    let intervalTimer: ReturnType<typeof setInterval> | null = null;
    let cancelled = false;

    const runCheck = async () => {
      if (cancelled) return;
      try {
        const cfg = await invoke<any>('get_storage_config').catch(() => null);
        if (!cfg?.auto_update_enabled) return;
        await checkForUpdates(true, true);
        const current = getUpdaterState();
        const canAutoDownload =
          current.installable &&
          current.updateAvailable;
        const ignoredVersion = ignoredUpdateVersionRef.current;
        const currentVersion = current.manifest?.version || null;
        const isIgnoredVersion = !!ignoredVersion && ignoredVersion === currentVersion;
        if (canAutoDownload && !isIgnoredVersion) {
          await downloadUpdateIfAvailable(true);
        }
      } catch (e) {
        console.error('Auto update scheduler failed:', e);
      }
    };

    const setupScheduler = async () => {
      const cfg = await invoke<any>('get_storage_config').catch(() => null);
      if (!cfg?.auto_update_enabled) return;
      const minsRaw = Number(cfg.update_check_interval_minutes ?? 360);
      const intervalMins = Number.isFinite(minsRaw) ? Math.min(1440, Math.max(30, minsRaw)) : 360;
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
    if (!isTauri || onboardingStatus !== 'done') {
      return;
    }
    let intervalTimer: ReturnType<typeof setInterval> | null = null;
    let stopped = false;

    const run = async () => {
      if (stopped) return;
      try {
        const cfg = await invoke<any>('get_storage_config').catch(() => null);
        if (!cfg?.skills_sync_enabled) return;
        await invoke('skills_sync_now');
      } catch (e) {
        console.error('skills sync scheduler failed', e);
      }
    };

    const setup = async () => {
      const cfg = await invoke<any>('get_storage_config').catch(() => null);
      if (!cfg?.skills_sync_enabled) return;
      const minsRaw = Number(cfg.skills_sync_interval_minutes ?? 60);
      const intervalMins = Number.isFinite(minsRaw) ? Math.min(1440, Math.max(5, minsRaw)) : 60;
      intervalTimer = setInterval(run, intervalMins * 60_000);
    };

    setup();
    return () => {
      stopped = true;
      if (intervalTimer) clearInterval(intervalTimer);
    };
  }, [isTauri, onboardingStatus]);

  useEffect(() => {
    if (!isTauri || onboardingStatus !== 'done') {
      return;
    }
    let initialTimer: ReturnType<typeof setTimeout> | null = null;
    let intervalTimer: ReturnType<typeof setInterval> | null = null;
    let stopped = false;

    const run = async () => {
      if (stopped) return;
      try {
        const cfg = await invoke<any>('get_storage_config').catch(() => null);
        if (!cfg?.ai_news_enabled) return;
        await invoke('ai_news_sync_now');
      } catch (e) {
        console.error('ai news sync scheduler failed', e);
      }
    };

    const setup = async () => {
      const cfg = await invoke<any>('get_storage_config').catch(() => null);
      if (!cfg?.ai_news_enabled) return;
      const minsRaw = Number(cfg.ai_news_sync_interval_minutes ?? 60);
      const intervalMins = Number.isFinite(minsRaw) ? Math.min(1440, Math.max(5, minsRaw)) : 60;
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
    if (!isTauri || onboardingStatus !== 'done') {
      return;
    }
    let intervalTimer: ReturnType<typeof setInterval> | null = null;
    let stopped = false;

    const run = async () => {
      if (stopped) return;
      try {
        const cfg = await invoke<any>('get_storage_config').catch(() => null);
        if (!cfg?.subagents_sync_enabled) return;
        await invoke('subagents_sync_now');
      } catch (e) {
        console.error('subagents sync scheduler failed', e);
      }
    };

    const setup = async () => {
      const cfg = await invoke<any>('get_storage_config').catch(() => null);
      if (!cfg?.subagents_sync_enabled) return;
      const minsRaw = Number(cfg.subagents_sync_interval_minutes ?? 60);
      const intervalMins = Number.isFinite(minsRaw) ? Math.min(1440, Math.max(5, minsRaw)) : 60;
      intervalTimer = setInterval(run, intervalMins * 60_000);
    };

    setup();
    return () => {
      stopped = true;
      if (intervalTimer) clearInterval(intervalTimer);
    };
  }, [isTauri, onboardingStatus]);

  const navigation = useMemo(() => [
    { id: 'launcher', name: t('launcher'), icon: Rocket, count: counts.launcher },
    { id: 'ai-sessions', name: t('aiSessions'), icon: Terminal, count: counts.sessions },
    { id: 'ai-environments', name: t('aiEnvironments'), icon: Cpu, count: counts.environments },
    { id: 'ai-news', name: t('aiNews', 'AI News'), icon: Newspaper },
    { id: 'skills', name: t('skills', 'Skills'), icon: Sparkles, count: counts.skills },
    { id: 'subagents', name: t('subagents', 'Subagents'), icon: Bot, count: counts.subagents },
    { id: 'mcp-servers', name: 'MCP Servers', icon: MCPIcon, count: counts.mcpServers },
    { id: 'ssh', name: t('sshServers'), icon: Server, count: counts.ssh },
    { id: 'snippets', name: t('snippets'), icon: Code2, count: counts.snippets },
    { id: 'bookmarks', name: t('bookmarks'), icon: Star, count: counts.bookmarks },
    { id: 'notes', name: t('notes'), icon: StickyNote, count: counts.notes },
    { id: 'mail', name: t('mail'), icon: MailIcon, count: counts.mail > 0 ? counts.mail : undefined },
  ], [t, counts]);

  const toggleLanguage = async () => {
    const newLang = i18n.language === 'zh' ? 'en' : 'zh';
    await i18n.changeLanguage(newLang);
    
    if (isTauri) {
      try {
        const cfg = await invoke<any>('get_storage_config');
        await invoke('save_storage_config', { config: { ...cfg, language: newLang } });
        await invoke('update_tray_menu', { lang: newLang });
      } catch (e) {
        console.error('Failed to save language preference:', e);
      }
    }
  };

  const cycleTheme = () => {
    if (theme === 'system') setTheme('dark');
    else if (theme === 'dark') setTheme('light');
    else setTheme('system');
  };

  const openGithubRepo = async () => {
    const repoUrl = 'https://github.com/minbox-projects/one-space';
    if (isTauri) {
      await open(repoUrl);
      return;
    }
    window.open(repoUrl, '_blank', 'noopener,noreferrer');
  };

  const copySyncError = () => {
    if (syncError) {
      navigator.clipboard.writeText(syncError);
      const originalError = syncError;
      setSyncError(t('copied', 'Copied!'));
      setTimeout(() => setSyncError(originalError), 2000);
    }
  };

  const ThemeIcon = theme === 'system' ? Monitor : theme === 'dark' ? Moon : Sun;
  const themeLabel = theme === 'system' ? t('themeSystem') : theme === 'dark' ? t('themeDark') : t('themeLight');
  const currentUpdateVersion = updaterManifest?.version ?? '';
  const hasUpdateCandidate = !!currentUpdateVersion && (
    updaterStatus === 'available' ||
    updaterStatus === 'downloading' ||
    updaterStatus === 'downloaded' ||
    updaterStatus === 'installing'
  );
  const isCurrentVersionIgnored = !!currentUpdateVersion && currentUpdateVersion === ignoredUpdateVersion;
  const showUpdateIndicator = hasUpdateCandidate && !isCurrentVersionIgnored;
  const updateIndicatorTitle = t('newVersionDetected', { version: currentUpdateVersion });

  useEffect(() => {
    if (!showUpdateIndicator) {
      setUpdateDialogOpen(false);
    }
  }, [showUpdateIndicator]);

  const openReleasesPage = async () => {
    const releasesUrl = 'https://github.com/minbox-projects/one-space/releases';
    if (isTauri) {
      await open(releasesUrl);
      return;
    }
    window.open(releasesUrl, '_blank', 'noopener,noreferrer');
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
      if (updaterStatus === 'downloaded') {
        await installDownloadedUpdate();
        return;
      }
      await installUpdate();
      const current = getUpdaterState();
      if (current.status === 'downloaded') {
        await installDownloadedUpdate();
      }
    } catch (e) {
      console.error('Failed to run update flow:', e);
    }
  };

  const handleIgnoreVersion = async () => {
    if (!updaterManifest?.version) {
      return;
    }
    try {
      const cfg = await invoke<any>('get_storage_config');
      await invoke('save_storage_config', {
        config: {
          ...cfg,
          update_ignored_version: updaterManifest.version,
        },
      });
      setIgnoredUpdateVersion(updaterManifest.version);
      setUpdateDialogOpen(false);
    } catch (e) {
      console.error('Failed to save ignored update version:', e);
    }
  };

  const renderUpdateIndicatorIcon = () => {
    if (updaterStatus === 'downloading' || updaterStatus === 'installing') {
      return <Loader2 className="w-5 h-5 animate-spin" />;
    }
    if (updaterStatus === 'downloaded') {
      return <ArrowUpCircle className="w-5 h-5 animate-pulse" />;
    }
    return <ArrowUpCircle className="w-5 h-5 animate-bounce" />;
  };

  const toggleFishPond = () => {
    if (activeTab === 'fish-pond') {
      const fallbackTab = fishPondPreviousTab !== 'fish-pond' ? fishPondPreviousTab : 'launcher';
      setActiveTab(fallbackTab);
      return;
    }
    setFishPondPreviousTab(activeTab);
    setActiveTab('fish-pond');
  };

  const resolvedTheme = useMemo(() => theme === 'system' 
    ? (window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light")
    : theme, [theme]);

  // If we are in quick-ai view, render only that component
  if (isQuickAiView) {
    return <QuickAiSessionBar />;
  }

  if (onboardingStatus === 'checking') {
    return (
      <div className="h-screen w-screen bg-background text-foreground flex items-center justify-center">
        <div className="text-sm text-muted-foreground">{t('loading', 'Loading...')}</div>
      </div>
    );
  }

  if (onboardingStatus === 'required') {
    return (
      <OnboardingWizard
        onComplete={(nextStorageType) => {
          setStorageType(nextStorageType);
          setOnboardingStatus('done');
        }}
      />
    );
  }

  const renderContent = () => {
    const shouldRenderTab = (tabId: string) => activeTab === tabId || mountedTabs.has(tabId);
    return (
      <div className="h-full relative">
        {shouldRenderTab('launcher') && <div className={activeTab === 'launcher' ? 'h-full' : 'hidden'}><Launcher /></div>}
        {shouldRenderTab('ai-sessions') && <div className={activeTab === 'ai-sessions' ? 'h-full' : 'hidden'}>
          <AiSessions onNavigate={(tab, hash) => {
            setActiveTab(tab);
            if (hash) window.location.hash = hash;
          }} />
        </div>}
        {shouldRenderTab('ai-environments') && <div className={activeTab === 'ai-environments' ? 'h-full' : 'hidden'}>
          <AiEnvironments isVisible={activeTab === 'ai-environments'} />
        </div>}
        {shouldRenderTab('ai-news') && <div className={activeTab === 'ai-news' ? 'h-full' : 'hidden'}>
          <AiNews isVisible={activeTab === 'ai-news'} />
        </div>}
        {shouldRenderTab('skills') && <div className={activeTab === 'skills' ? 'h-full' : 'hidden'}>
          <Skills isVisible={activeTab === 'skills'} />
        </div>}
        {shouldRenderTab('subagents') && <div className={activeTab === 'subagents' ? 'h-full' : 'hidden'}>
          <Subagents isVisible={activeTab === 'subagents'} />
        </div>}
        {shouldRenderTab('mcp-servers') && <div className={activeTab === 'mcp-servers' ? 'h-full' : 'hidden'}>
          <MCPServers isVisible={activeTab === 'mcp-servers'} />
        </div>}
        {shouldRenderTab('ssh') && <div className={activeTab === 'ssh' ? 'h-full' : 'hidden'}><SshServers /></div>}
        {shouldRenderTab('snippets') && <div className={activeTab === 'snippets' ? 'h-full' : 'hidden'}><Snippets /></div>}
        {shouldRenderTab('bookmarks') && <div className={activeTab === 'bookmarks' ? 'h-full' : 'hidden'}><Bookmarks /></div>}
        {shouldRenderTab('notes') && <div className={activeTab === 'notes' ? 'h-full' : 'hidden'}><Notes /></div>}
        {shouldRenderTab('documentation') && <div className={activeTab === 'documentation' ? 'h-full' : 'hidden'}><Documentation /></div>}
        {shouldRenderTab('cloud') && <div className={activeTab === 'cloud' ? 'h-full' : 'hidden'}><CloudDrive /></div>}
        {shouldRenderTab('mail') && <div className={activeTab === 'mail' ? 'h-full' : 'hidden'}>
          <Mail isVisible={activeTab === 'mail'} />
        </div>}
        {shouldRenderTab('fish-pond') && <div className={activeTab === 'fish-pond' ? 'h-full' : 'hidden'}>
          <FishPond />
        </div>}
        {shouldRenderTab('settings') && <div className={activeTab === 'settings' ? 'h-full' : 'hidden'}>
          <SettingsView 
            initialTab={settingsInitialTab} 
            onBack={() => {
              setActiveTab(previousTab);
              setSettingsInitialTab('storage');
              scheduleLoadCounts(0);
            }} 
          />
        </div>}
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
              src={resolvedTheme === 'dark' ? logoWhite : logoBlack} 
              alt="OneSpace" 
              className="w-5 h-5"
            />
            <span className="text-lg">OneSpace</span>
          </div>
        </div>
        
        <div className="flex-1 overflow-y-auto py-4 px-3 space-y-1">
          {navigation.map((item: any) => (
            <button
              key={item.id}
              onClick={() => setActiveTab(item.id)}
              className={`w-full flex items-center justify-between px-3 py-2 rounded-md text-sm transition-colors ${
                activeTab === item.id 
                  ? 'bg-primary text-primary-foreground font-medium shadow-sm' 
                  : 'hover:bg-muted text-muted-foreground hover:text-foreground'
              }`}
            >
              <div className="flex items-center gap-3">
                <item.icon className={`w-4 h-4 ${activeTab === item.id ? 'animate-pulse' : ''}`} />
                <span>{item.name}</span>
              </div>
              {item.count !== undefined && (
                <span className={`text-[10px] px-1.5 py-0.5 rounded-full font-mono ${
                  activeTab === item.id 
                    ? 'bg-primary-foreground/20 text-primary-foreground' 
                    : 'bg-muted-foreground/10 text-muted-foreground'
                }`}>
                  {item.count}
                </span>
              )}
            </button>
          ))}
        </div>

        <div className="p-3 border-t space-y-1">
          <button 
            onClick={() => {
              setPreviousTab(activeTab);
              setActiveTab('settings');
            }}
            className={`w-full flex items-center gap-3 px-3 py-2 rounded-md text-sm transition-colors ${
              activeTab === 'settings' 
                ? 'bg-primary/10 text-primary font-medium' 
                : 'text-muted-foreground hover:bg-muted hover:text-foreground'
            }`}
          >
            <Settings className={`w-4 h-4 ${activeTab === 'settings' ? 'animate-pulse' : ''}`} />
            {t('settings')}
          </button>
          <button 
            onClick={() => setActiveTab('documentation')}
            className={`w-full flex items-center gap-3 px-3 py-2 rounded-md text-sm transition-colors ${
              activeTab === 'documentation' 
                ? 'bg-primary/10 text-primary font-medium' 
                : 'text-muted-foreground hover:bg-muted hover:text-foreground'
            }`}
          >
            <BookOpen className={`w-4 h-4 ${activeTab === 'documentation' ? 'animate-pulse' : ''}`} />
            {t('usageDocs')}
          </button>
          <button 
            onClick={() => setAboutOpen(true)}
            className="w-full flex items-center gap-3 px-3 py-2 rounded-md text-sm text-muted-foreground hover:bg-muted hover:text-foreground transition-colors"
          >
            <Info className="w-4 h-4" />
            {t('about')}
          </button>
        </div>
      </div>

      <div className="flex-1 flex flex-col overflow-hidden relative bg-background">
        {activeTab !== 'settings' && (
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
                  <span className="group-hover:text-foreground transition-colors">{t('search')}...</span>
                </div>
                <kbd className="hidden sm:inline-flex h-5 items-center gap-1 rounded border bg-background/50 px-1.5 font-mono text-[10px] font-medium opacity-60">
                  <span className="text-xs">⌘</span>K
                </kbd>
              </button>

              {syncStatus !== 'idle' && (
                <div className="flex items-center gap-2">
                  {syncStatus === 'pulling' && (
                    <div className="flex items-center gap-2 px-2.5 py-1 bg-primary/5 rounded-full border border-primary/10 animate-pulse">
                      <Loader2 className="w-3 h-3 text-primary animate-spin" />
                      <span className="text-[10px] font-semibold text-primary/80 uppercase tracking-wider">
                        {storageType === 'git' ? t('syncingToGit', 'Syncing to Git') : storageType === 'icloud' ? t('savingToICloud', 'Syncing to iCloud') : t('savingLocally')}
                      </span>
                    </div>
                  )}
                  {syncStatus === 'pushing' && (
                    <div className="flex items-center gap-2 px-2.5 py-1 bg-primary/5 rounded-full border border-primary/10 animate-pulse">
                      <Loader2 className="w-3 h-3 text-primary animate-spin" />
                      <span className="text-[10px] font-semibold text-primary/80 uppercase tracking-wider">
                        {storageType === 'git' ? t('syncingToGit', 'Syncing to Git') : storageType === 'icloud' ? t('savingToICloud', 'Syncing to iCloud') : t('savingLocally')}
                      </span>
                    </div>
                  )}
                  {syncStatus === 'success' && (
                    <div className="flex items-center gap-2 px-2.5 py-1 bg-green-500/5 rounded-full border border-green-500/20">
                      <CheckCircle2 className="w-3 h-3 text-green-500" />
                      <span className="text-[10px] font-semibold text-green-500/80 uppercase tracking-wider">
                        {storageType === 'git' ? t('syncedToGit') : storageType === 'icloud' ? t('savedToICloud', 'Saved to iCloud') : t('savedLocally')}
                      </span>
                    </div>
                  )}
                  {syncStatus === 'error' && (
                    <div 
                      className="group relative flex items-center gap-2 px-2.5 py-1 bg-destructive/5 rounded-full border border-destructive/20 cursor-pointer transition-colors hover:bg-destructive/10"
                      onClick={copySyncError}
                    >
                      <AlertCircle className="w-3 h-3 text-destructive" />
                      <span className="text-[10px] font-semibold text-destructive/80 uppercase tracking-wider">{t('syncError', 'Sync Error')}</span>
                      <div className="absolute left-0 top-full mt-2 w-64 p-2 bg-destructive text-destructive-foreground text-[10px] rounded-md shadow-xl opacity-0 group-hover:opacity-100 transition-opacity z-50 select-text pointer-events-auto border border-destructive/20">
                        <div className="flex flex-col gap-1">
                          <span className="font-bold border-b border-destructive-foreground/20 pb-1 mb-1 flex justify-between items-center">
                            {t('syncErrorInfo', 'Error Details')}
                            <span className="text-[8px] opacity-70 uppercase tracking-widest bg-destructive-foreground/10 px-1 rounded">{t('clickToCopy', 'Click to copy')}</span>
                          </span>
                          <span className="break-words line-clamp-4 leading-relaxed">{syncError}</span>
                        </div>
                      </div>
                    </div>
                  )}
                </div>
              )}
            </div>
            
            <div className="flex items-center gap-1">
              <button
                onClick={toggleFishPond}
                className={`p-2.5 rounded-md transition-colors ${
                  activeTab === 'fish-pond' 
                    ? 'bg-primary/10 text-primary' 
                    : 'text-muted-foreground hover:bg-muted hover:text-foreground'
                }`}
                title={t('fishPond', 'Fish Pond')}
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
                title={t('toggleLanguage')}
              >
                {i18n.language === 'zh' ? (
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

        <main className={`flex-1 overflow-y-auto ${activeTab === 'settings' ? 'p-0' : 'p-6'}`}>
          {renderContent()}
        </main>
      </div>

      <OmniSearch
        open={omniOpen}
        setOpen={setOmniOpen}
        onNavigate={(tab) => {
          setActiveTab(tab);
        }}
      />
      <AboutModal open={aboutOpen} onClose={() => setAboutOpen(false)} />
      <UpdateUpgradeModal
        open={updateDialogOpen}
        onClose={() => setUpdateDialogOpen(false)}
        currentVersion={updaterManifest?.currentVersion || '-'}
        latestVersion={updaterManifest?.version || '-'}
        releaseNotes={updaterManifest?.body || ''}
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
