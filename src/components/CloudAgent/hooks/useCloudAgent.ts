import { useState, useEffect, useCallback, useRef } from "react";
import { sshHostsList } from "@/lib/sshTunnels";
import {
  cloudAgentGetSettings,
  cloudAgentGetStatus,
  cloudAgentListSessions,
  cloudAgentListWorktrees,
  cloudAgentListTasks,
  cloudAgentCreateSession,
  cloudAgentKillSession,
  cloudAgentLaunchTerminal,
  cloudAgentSyncGateway,
  cloudAgentGetInstallationInfo,
  CLOUD_AGENT_SESSIONS_COUNT_EVENT,
  CLOUD_AGENT_SETTINGS_UPDATED_EVENT,
  DEFAULT_CLOUD_AGENT_SETTINGS,
  type CloudAgentSettings,
  type CloudAgentStatus,
  type CloudSession,
  type CloudWorktree,
  type CloudAgentTask,
  type GatewaySyncResult,
  type CloudAgentInstallationInfo,
  type CloudAgentDeploymentResult,
} from "@/lib/cloudAgent";

export type CloudAgentTab = "sessions" | "worktrees" | "pipeline" | "gateway";

export interface CloudAgentHostOption {
  id: string;
  label: string;
}

export function useCloudAgent(
  pushToast?: (type: "success" | "error", text: string) => void
) {
  const [activeTab, setActiveTab] = useState<CloudAgentTab>("sessions");
  const [settings, setSettings] = useState<CloudAgentSettings>(
    DEFAULT_CLOUD_AGENT_SETTINGS
  );
  const [hosts, setHosts] = useState<CloudAgentHostOption[]>([]);
  const [selectedHost, setSelectedHost] = useState<string>("");
  const [status, setStatus] = useState<CloudAgentStatus | null>(null);
  const [sessions, setSessions] = useState<CloudSession[]>([]);
  const [worktrees, setWorktrees] = useState<CloudWorktree[]>([]);
  const [tasks, setTasks] = useState<CloudAgentTask[]>([]);
  const [installation, setInstallation] =
    useState<CloudAgentInstallationInfo | null>(null);
  const [installationLoading, setInstallationLoading] = useState(false);
  const [installationError, setInstallationError] = useState<string | null>(
    null
  );
  const [deploymentActive, setDeploymentActive] = useState(false);
  const [loading, setLoading] = useState(false);
  const [showNewTaskModal, setShowNewTaskModal] = useState(false);

  const generationRef = useRef(0);
  const selectedHostRef = useRef("");
  const deploymentActiveRef = useRef(false);

  const runReads = useCallback(async (host: string, generation: number) => {
    if (!host.trim()) {
      setStatus(null);
      setSessions([]);
      setWorktrees([]);
      setTasks([]);
      setLoading(false);
      return;
    }

    setLoading(true);
    const [statusResult, sessionsResult, worktreesResult, tasksResult] =
      await Promise.allSettled([
        cloudAgentGetStatus(host),
        cloudAgentListSessions(host),
        cloudAgentListWorktrees(host),
        cloudAgentListTasks(host),
      ]);

    if (
      generation !== generationRef.current ||
      selectedHostRef.current !== host
    ) {
      return;
    }

    setStatus(
      statusResult.status === "fulfilled" ? statusResult.value : null
    );
    setSessions(
      sessionsResult.status === "fulfilled" ? sessionsResult.value : []
    );
    setWorktrees(
      worktreesResult.status === "fulfilled" ? worktreesResult.value : []
    );
    setTasks(tasksResult.status === "fulfilled" ? tasksResult.value : []);
    setLoading(false);
  }, []);

  const loadInstallation = useCallback(
    async (host: string, generation: number) => {
      if (!host.trim()) {
        setInstallation(null);
        setInstallationError(null);
        setInstallationLoading(false);
        return;
      }

      setInstallationLoading(true);
      setInstallationError(null);
      try {
        const info = await cloudAgentGetInstallationInfo(host);
        if (
          generation !== generationRef.current ||
          selectedHostRef.current !== host
        ) {
          return;
        }
        setInstallation(info);
      } catch (error: unknown) {
        if (
          generation !== generationRef.current ||
          selectedHostRef.current !== host
        ) {
          return;
        }
        setInstallation(null);
        setInstallationError(
          error instanceof Error ? error.message : String(error)
        );
      } finally {
        if (
          generation === generationRef.current &&
          selectedHostRef.current === host
        ) {
          setInstallationLoading(false);
        }
      }
    },
    []
  );

  /**
   * Apply a host selection synchronously: increment the read generation first,
   * clear the previous host's data, then start one guarded read pass.
   */
  const applyHost = useCallback(
    (nextHost: string) => {
      const host = nextHost?.trim() ?? "";
      generationRef.current += 1;
      const generation = generationRef.current;
      selectedHostRef.current = host;
      setSelectedHost(host);
      setStatus(null);
      setSessions([]);
      setWorktrees([]);
      setTasks([]);
      setInstallation(null);
      setInstallationError(null);
      setLoading(Boolean(host));
      setInstallationLoading(Boolean(host));

      if (!host) {
        setLoading(false);
        setInstallationLoading(false);
        return;
      }

      void runReads(host, generation);
      void loadInstallation(host, generation);

    },
    [runReads, loadInstallation]
  );

  // 加载初始设置与 SSH 主机列表，推导首选主机
  useEffect(() => {
    let cancelled = false;

    Promise.allSettled([
      cloudAgentGetSettings(),
      sshHostsList<{ name: string; host_name: string }[]>(),
    ]).then(([settingsRes, hostsRes]) => {
      if (cancelled) return;

      const loadedSettings =
        settingsRes.status === "fulfilled"
          ? settingsRes.value
          : DEFAULT_CLOUD_AGENT_SETTINGS;
      const hostList =
        hostsRes.status === "fulfilled" && Array.isArray(hostsRes.value)
          ? hostsRes.value
          : [];

      setSettings(loadedSettings);
      setHosts(
        hostList
          .filter((host) => Boolean(host.name))
          .map((host) => ({ id: host.name, label: host.name }))
      );

      let initialHost = "";
      if (loadedSettings.defaultHost && loadedSettings.defaultHost.trim()) {
        initialHost = loadedSettings.defaultHost.trim();
      } else if (hostList.length > 0 && hostList[0].name) {
        initialHost = hostList[0].name;
      }

      applyHost(initialHost);
    });

    return () => {
      cancelled = true;
    };
  }, [applyHost]);

  // 监听全局设置保存事件，动态感知并在保存了默认主机后实时联动更新
  useEffect(() => {
    const handleSettingsUpdated = (e: Event) => {
      const customEvent = e as CustomEvent<CloudAgentSettings>;
      if (customEvent.detail) {
        const nextSettings = customEvent.detail;
        setSettings(nextSettings);
        if (nextSettings.defaultHost && nextSettings.defaultHost.trim()) {
          applyHost(nextSettings.defaultHost.trim());
        }
      }
    };

    window.addEventListener(
      CLOUD_AGENT_SETTINGS_UPDATED_EVENT,
      handleSettingsUpdated
    );
    return () => {
      window.removeEventListener(
        CLOUD_AGENT_SETTINGS_UPDATED_EVENT,
        handleSettingsUpdated
      );
    };
  }, [applyHost]);

  // 刷新当前主机数据（保留当前选中主机，不重新执行安装检查）
  const refreshAll = useCallback(
    (hostOverride?: string) => {
      const host = (hostOverride ?? selectedHostRef.current).trim();
      if (!host) {
        setStatus(null);
        setSessions([]);
        setWorktrees([]);
        setTasks([]);
        setLoading(false);
        return Promise.resolve();
      }
      return runReads(host, generationRef.current);
    },
    [runReads]
  );

  // 只读取当前主机的在线状态，绝不触发列表读取或安装
  const loadStatus = useCallback(async (host: string, generation: number) => {
    if (!host.trim()) return;
    try {
      const nextStatus = await cloudAgentGetStatus(host);
      if (
        generation !== generationRef.current ||
        selectedHostRef.current !== host
      ) {
        return;
      }
      setStatus(nextStatus);
    } catch {
      if (
        generation !== generationRef.current ||
        selectedHostRef.current !== host
      ) {
        return;
      }
      setStatus(null);
    }
  }, []);

  // 重新读取当前主机的安装信息与在线状态，绝不触发安装
  const retest = useCallback(() => {
    const host = selectedHostRef.current.trim();
    if (!host) return;
    const generation = generationRef.current;
    void loadStatus(host, generation);
    void loadInstallation(host, generation);
  }, [loadStatus, loadInstallation]);

  // 会话列表更新时，通知全局同步活跃会话总数
  useEffect(() => {
    const activeCount = sessions.filter((s) => s.is_active !== false).length;
    window.dispatchEvent(
      new CustomEvent(CLOUD_AGENT_SESSIONS_COUNT_EVENT, {
        detail: { count: activeCount },
      })
    );
  }, [sessions]);

  // 定时自动轮询；部署期间跳过，部署结束后恢复正常轮询
  useEffect(() => {
    if (!selectedHost || !selectedHost.trim()) return;

    const sec = Math.max(3, settings.autoRefreshIntervalSeconds || 10);
    const interval = setInterval(() => {
      if (deploymentActiveRef.current) return;
      const host = selectedHostRef.current.trim();
      if (!host) return;
      void runReads(host, generationRef.current);
    }, sec * 1000);

    return () => {
      clearInterval(interval);
    };
  }, [selectedHost, runReads, settings.autoRefreshIntervalSeconds]);

  const handleDeploymentStart = useCallback(() => {
    deploymentActiveRef.current = true;
    setDeploymentActive(true);
  }, []);

  const handleDeploymentEnd = useCallback(() => {
    deploymentActiveRef.current = false;
    setDeploymentActive(false);
  }, []);

  const handleDeploymentInstalled = useCallback(
    (result: CloudAgentDeploymentResult) => {
      const target = result.host?.trim();
      if (!target) return;
      applyHost(target);
    },
    [applyHost]
  );

  // 恢复会话 / 调起终端
  const launchTerminal = useCallback(
    async (target: string, branch?: string, yolo?: boolean, extra?: string) => {
      try {
        await cloudAgentLaunchTerminal({
          host: selectedHost,
          target,
          branch,
          useYolo: yolo !== undefined ? yolo : settings.opencodeYoloMode,
          extraArgs: extra !== undefined ? extra : settings.opencodeExtraArgs,
        });
        pushToast?.("success", `已在本地终端唤起并直连会话 [${target}]`);
      } catch (err: any) {
        pushToast?.("error", `调起终端失败: ${err?.message || err}`);
        throw err;
      }
    },
    [selectedHost, settings, pushToast]
  );

  // 终止会话
  const killSession = useCallback(
    async (sessionName: string) => {
      try {
        const ok = await cloudAgentKillSession(sessionName, selectedHost);
        if (ok) {
          pushToast?.("success", `会话 [${sessionName}] 已成功终止`);
          void refreshAll();
        } else {
          pushToast?.("error", `会话 [${sessionName}] 终止失败`);
        }
      } catch (err: any) {
        pushToast?.("error", `终止失败: ${err?.message || err}`);
      }
    },
    [selectedHost, refreshAll, pushToast]
  );

  // 同步网关
  const syncGateway = useCallback(async (): Promise<
    GatewaySyncResult | undefined
  > => {
    try {
      const res = await cloudAgentSyncGateway(selectedHost);
      pushToast?.("success", res.message);
      void refreshAll();
      return res;
    } catch (err: any) {
      pushToast?.("error", `同步网关失败: ${err?.message || err}`);
    }
  }, [selectedHost, refreshAll, pushToast]);

  // 创建新会话并唤起终端
  const createAndLaunchSession = useCallback(
    async (data: {
      repo: string;
      branch?: string;
      useYolo: boolean;
      extraArgs: string;
    }) => {
      try {
        // 先在云端创建会话与 Worktree
        const res = await cloudAgentCreateSession({
          host: selectedHost,
          repo: data.repo,
          branch: data.branch,
          useYolo: data.useYolo,
          extraArgs: data.extraArgs,
        });

        // 成功后调起本地终端直连
        const targetSession = res.session_name || data.repo;
        await cloudAgentLaunchTerminal({
          host: selectedHost,
          target: targetSession,
          branch: data.branch,
          useYolo: data.useYolo,
          extraArgs: data.extraArgs,
        });

        pushToast?.("success", `特性会话创建成功，已打开本地终端直连`);
        void refreshAll();
      } catch (err: any) {
        pushToast?.("error", `创建会话失败: ${err?.message || err}`);
        throw err;
      }
    },
    [selectedHost, refreshAll, pushToast]
  );

  return {
    activeTab,
    setActiveTab,
    settings,
    setSettings,
    hosts,
    selectedHost,
    setSelectedHost: applyHost,
    status,
    sessions,
    worktrees,
    tasks,
    installation,
    installationLoading,
    installationError,
    deploymentActive,
    loading,
    refreshAll,
    retest,
    handleDeploymentStart,
    handleDeploymentEnd,
    handleDeploymentInstalled,
    launchTerminal,
    killSession,
    syncGateway,
    createAndLaunchSession,
    showNewTaskModal,
    setShowNewTaskModal,
  };
}
