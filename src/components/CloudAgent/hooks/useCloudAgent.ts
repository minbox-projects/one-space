import { useState, useEffect, useCallback, useRef } from "react";
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
  const [selectedHost, setSelectedHost] = useState<string>("dmit-server");
  const [status, setStatus] = useState<CloudAgentStatus | null>(null);
  const [sessions, setSessions] = useState<CloudSession[]>([]);
  const [worktrees, setWorktrees] = useState<CloudWorktree[]>([]);
  const [tasks, setTasks] = useState<CloudAgentTask[]>([]);
  const [loading, setLoading] = useState(false);
  const [showNewTaskModal, setShowNewTaskModal] = useState(false);

  // 加载初始设置
  useEffect(() => {
    cloudAgentGetSettings()
      .then((s) => {
        setSettings(s);
        if (s.defaultHost) {
          setSelectedHost(s.defaultHost);
        }
      })
      .catch(() => {});
  }, []);

  // 刷新当前主机数据
  const refreshAll = useCallback(
    async (hostOverride?: string) => {
      const host = hostOverride || selectedHost;
      if (!host) return;

      try {
        setLoading(true);
        const [st, ss, wt, tk] = await Promise.allSettled([
          cloudAgentGetStatus(host),
          cloudAgentListSessions(host),
          cloudAgentListWorktrees(host),
          cloudAgentListTasks(host),
        ]);

        if (st.status === "fulfilled") setStatus(st.value);
        if (ss.status === "fulfilled") setSessions(ss.value);
        if (wt.status === "fulfilled") setWorktrees(wt.value);
        if (tk.status === "fulfilled") setTasks(tk.value);
      } finally {
        setLoading(false);
      }
    },
    [selectedHost]
  );

  // 主机变更时刷新
  useEffect(() => {
    refreshAll();
  }, [refreshAll]);

  // 定时自动轮询
  const pollIntervalRef = useRef<NodeJS.Timeout | null>(null);
  useEffect(() => {
    const sec = Math.max(3, settings.autoRefreshIntervalSeconds || 10);
    pollIntervalRef.current = setInterval(() => {
      refreshAll();
    }, sec * 1000);

    return () => {
      if (pollIntervalRef.current) clearInterval(pollIntervalRef.current);
    };
  }, [refreshAll, settings.autoRefreshIntervalSeconds]);

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
    showNewTaskModal,
    setShowNewTaskModal,
  };
}
