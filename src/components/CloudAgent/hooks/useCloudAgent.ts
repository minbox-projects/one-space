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
  cloudAgentPlanAttach,
  cloudAgentPlanCancel,
  CLOUD_AGENT_SESSIONS_COUNT_EVENT,
  CLOUD_AGENT_SETTINGS_UPDATED_EVENT,
  DEFAULT_CLOUD_AGENT_SETTINGS,
  type CloudAgentSettings,
  type CloudAgentStatus,
  type CloudSession,
  type CloudWorktree,
  type CloudAgentTask,
  type GatewaySyncResult,
} from "@/lib/cloudAgent";

export type CloudAgentTab = "sessions" | "worktrees" | "pipeline" | "gateway";

export function useCloudAgent(
  pushToast?: (type: "success" | "error", text: string) => void
) {
  const [activeTab, setActiveTab] = useState<CloudAgentTab>("sessions");
  const [settings, setSettings] = useState<CloudAgentSettings>(
    DEFAULT_CLOUD_AGENT_SETTINGS
  );
  const [selectedHost, setSelectedHost] = useState<string>("");
  const [status, setStatus] = useState<CloudAgentStatus | null>(null);
  const [sessions, setSessions] = useState<CloudSession[]>([]);
  const [worktrees, setWorktrees] = useState<CloudWorktree[]>([]);
  const [tasks, setTasks] = useState<CloudAgentTask[]>([]);
  const [loading, setLoading] = useState(false);
  const [showNewTaskModal, setShowNewTaskModal] = useState(false);
  const [hasInitialized, setHasInitialized] = useState(false);

  const requestIdRef = useRef<number>(0);
  const currentHostRef = useRef<string>("");

  // 手动切换选中的主机并立即清空当前显示的状态以防数据残留
  const changeSelectedHost = useCallback((newHost: string) => {
    setSelectedHost(newHost);
    setStatus(null);
    setSessions([]);
    setWorktrees([]);
    setTasks([]);
  }, []);

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
      const hosts =
        hostsRes.status === "fulfilled" && Array.isArray(hostsRes.value)
          ? hostsRes.value
          : [];

      setSettings(loadedSettings);

      // 计算首选主机：若 settings.defaultHost 非空则优先使用；
      // 若 defaultHost 为空但存在可用 SSH 主机，自动选择第 1 个；否则置空
      let initialHost = "";
      if (loadedSettings.defaultHost && loadedSettings.defaultHost.trim()) {
        initialHost = loadedSettings.defaultHost.trim();
      } else if (hosts.length > 0 && hosts[0].name) {
        initialHost = hosts[0].name;
      }

      setSelectedHost(initialHost);
      setHasInitialized(true);
    });

    return () => {
      cancelled = true;
    };
  }, []);

  // 监听全局设置保存事件，动态感知并在保存了默认主机后实时联动更新
  useEffect(() => {
    const handleSettingsUpdated = (e: Event) => {
      const customEvent = e as CustomEvent<CloudAgentSettings>;
      if (customEvent.detail) {
        const nextSettings = customEvent.detail;
        setSettings(nextSettings);
        if (nextSettings.defaultHost && nextSettings.defaultHost.trim()) {
          changeSelectedHost(nextSettings.defaultHost.trim());
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
  }, [changeSelectedHost]);

  // 刷新当前主机数据（包含防竞态与异常时清空数据逻辑）
  const refreshAll = useCallback(
    async (hostOverride?: string) => {
      const host = hostOverride !== undefined ? hostOverride : selectedHost;
      if (!host || !host.trim()) {
        setStatus(null);
        setSessions([]);
        setWorktrees([]);
        setTasks([]);
        setLoading(false);
        return;
      }

      const reqId = ++requestIdRef.current;
      currentHostRef.current = host;
      setLoading(true);

      try {
        const [st, ss, wt, tk] = await Promise.allSettled([
          cloudAgentGetStatus(host),
          cloudAgentListSessions(host),
          cloudAgentListWorktrees(host),
          cloudAgentListTasks(host),
        ]);

        // 如果在此期间主机已改变或有新的请求发起，丢弃旧数据
        if (reqId !== requestIdRef.current || currentHostRef.current !== host) {
          return;
        }

        if (st.status === "fulfilled") {
          setStatus(st.value);
        } else {
          setStatus(null);
        }

        if (ss.status === "fulfilled") {
          setSessions(ss.value);
        } else {
          setSessions([]);
        }

        if (wt.status === "fulfilled") {
          setWorktrees(wt.value);
        } else {
          setWorktrees([]);
        }

        if (tk.status === "fulfilled") {
          setTasks(tk.value);
        } else {
          setTasks([]);
        }
      } finally {
        if (reqId === requestIdRef.current) {
          setLoading(false);
        }
      }
    },
    [selectedHost]
  );

  // 主机变更时刷新（初始化完成后才触发）
  useEffect(() => {
    if (!hasInitialized) return;
    refreshAll();
  }, [selectedHost, hasInitialized, refreshAll]);

  // 会话列表更新时，通知全局同步活跃会话总数
  useEffect(() => {
    const activeCount = sessions.filter((s) => s.is_active !== false).length;
    window.dispatchEvent(
      new CustomEvent(CLOUD_AGENT_SESSIONS_COUNT_EVENT, {
        detail: { count: activeCount },
      })
    );
  }, [sessions]);

  // 定时自动轮询
  const pollIntervalRef = useRef<NodeJS.Timeout | null>(null);
  useEffect(() => {
    if (!selectedHost || !selectedHost.trim()) return;

    const sec = Math.max(3, settings.autoRefreshIntervalSeconds || 10);
    pollIntervalRef.current = setInterval(() => {
      refreshAll();
    }, sec * 1000);

    return () => {
      if (pollIntervalRef.current) clearInterval(pollIntervalRef.current);
    };
  }, [selectedHost, refreshAll, settings.autoRefreshIntervalSeconds]);

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
          refreshAll();
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
      refreshAll();
      return res;
    } catch (err: any) {
      pushToast?.("error", `同步网关失败: ${err?.message || err}`);
    }
  }, [selectedHost, refreshAll, pushToast]);

  // 接管本地计划任务的实时会话
  const planAttach = useCallback(
    async (jobId: string) => {
      try {
        await cloudAgentPlanAttach(jobId, selectedHost);
        pushToast?.("success", `已接管计划任务 [${jobId}] 的实时会话`);
        refreshAll();
      } catch (err: any) {
        pushToast?.("error", `接管计划任务失败: ${err?.message || err}`);
      }
    },
    [selectedHost, refreshAll, pushToast]
  );

  // 取消本地计划任务
  const planCancel = useCallback(
    async (jobId: string) => {
      try {
        await cloudAgentPlanCancel(jobId, selectedHost);
        pushToast?.("success", `计划任务 [${jobId}] 已取消`);
        refreshAll();
      } catch (err: any) {
        pushToast?.("error", `取消计划任务失败: ${err?.message || err}`);
      }
    },
    [selectedHost, refreshAll, pushToast]
  );

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
        refreshAll();
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
    selectedHost,
    setSelectedHost,
    status,
    sessions,
    worktrees,
    tasks,
    loading,
    refreshAll,
    launchTerminal,
    killSession,
    syncGateway,
    createAndLaunchSession,
    planAttach,
    planCancel,
    showNewTaskModal,
    setShowNewTaskModal,
  };
}
