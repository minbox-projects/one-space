import { useState, useEffect } from "react";
import {
  CloudCog,
  Terminal,
  GitBranch,
  GitPullRequest,
  Network,
  RefreshCw,
  Sparkles,
  HardDrive,
  Settings,
  Server,
} from "lucide-react";
import type { TFunction } from "i18next";
import { CustomSelect } from "@/components/common/CustomSelect";
import { sshHostsList } from "@/lib/sshTunnels";
import type { CloudAgentTab } from "../hooks/useCloudAgent";
import type { CloudAgentStatus, CloudAgentSettings } from "@/lib/cloudAgent";

interface CloudAgentHeaderProps {
  t: TFunction;
  activeTab: CloudAgentTab;
  setActiveTab: (tab: CloudAgentTab) => void;
  selectedHost: string;
  setSelectedHost: (host: string) => void;
  status: CloudAgentStatus | null;
  settings: CloudAgentSettings;
  sessionsCount: number;
  worktreesCount: number;
  tasksCount: number;
  loading: boolean;
  onRefresh: () => void;
  onOpenNewTaskModal: () => void;
  onSyncGateway: () => void;
}

export function CloudAgentHeader({
  t,
  activeTab,
  setActiveTab,
  selectedHost,
  setSelectedHost,
  status,
  settings,
  sessionsCount,
  worktreesCount,
  tasksCount,
  loading,
  onRefresh,
  onOpenNewTaskModal,
  onSyncGateway,
}: CloudAgentHeaderProps) {
  const [hosts, setHosts] = useState<{ id: string; label: string }[]>([]);

  useEffect(() => {
    sshHostsList<{ name: string; host_name: string }[]>()
      .then((res) => {
        if (res && res.length > 0) {
          setHosts(res.map((h) => ({ id: h.name, label: h.name })));
        }
      })
      .catch(() => {});
  }, []);

  const disk = status?.disk;
  const isDiskAlert =
    disk && disk.use_percent >= (settings.diskAlertThresholdPercent || 80);

  return (
    <div className="border-b border-slate-200/80 bg-white/70 backdrop-blur-md dark:border-slate-800/80 dark:bg-slate-900/70">
      {/* 顶栏主标题与分段器 */}
      <div className="px-6 py-4 flex flex-wrap items-center justify-between gap-4">
        <div className="flex items-center gap-3">
          <div className="flex h-10 w-10 items-center justify-center rounded-xl bg-emerald-500/10 text-emerald-600 shadow-sm dark:bg-emerald-500/20 dark:text-emerald-400">
            <CloudCog className="h-6 w-6" />
          </div>
          <div>
            <h1 className="text-lg font-semibold tracking-tight text-slate-900 dark:text-white flex items-center gap-2">
              {t("cloudAgentTitle", "云端智能体 (Cloud Agent)")}
            </h1>
            <p className="text-xs text-slate-500 dark:text-slate-400">
              {t(
                "cloudAgentSubtitle",
                "长程编程断网保活 · 本地终端一键直连 · AI 网关双端同步 · Issues 自动实施流水线"
              )}
            </p>
          </div>
        </div>

        {/* 模式切换 Tab (参考 SSL 证书工具的样式) */}
        <div className="flex rounded-xl bg-slate-100 p-1 dark:bg-slate-800">
          <button
            type="button"
            onClick={() => setActiveTab("sessions")}
            className={`flex items-center gap-2 rounded-lg px-3.5 py-1.5 text-xs font-medium transition-all ${
              activeTab === "sessions"
                ? "bg-white text-emerald-600 shadow-sm dark:bg-slate-700 dark:text-emerald-400"
                : "text-slate-600 hover:text-slate-900 dark:text-slate-300 dark:hover:text-white"
            }`}
          >
            <Terminal className="h-3.5 w-3.5" />
            {t("activeSessionsTab", "活跃会话")}
            {sessionsCount > 0 && (
              <span className="ml-1 rounded-full bg-emerald-500/10 px-1.5 py-0.2 text-[10px] font-semibold text-emerald-600 dark:bg-emerald-500/20 dark:text-emerald-400">
                {sessionsCount}
              </span>
            )}
          </button>

          <button
            type="button"
            onClick={() => setActiveTab("worktrees")}
            className={`flex items-center gap-2 rounded-lg px-3.5 py-1.5 text-xs font-medium transition-all ${
              activeTab === "worktrees"
                ? "bg-white text-emerald-600 shadow-sm dark:bg-slate-700 dark:text-emerald-400"
                : "text-slate-600 hover:text-slate-900 dark:text-slate-300 dark:hover:text-white"
            }`}
          >
            <GitBranch className="h-3.5 w-3.5" />
            {t("worktreesHistoryTab", "历史工作区")}
            {worktreesCount > 0 && (
              <span className="ml-1 rounded-full bg-slate-200 dark:bg-slate-600 px-1.5 py-0.2 text-[10px] font-semibold text-slate-700 dark:text-slate-200">
                {worktreesCount}
              </span>
            )}
          </button>

          <button
            type="button"
            onClick={() => setActiveTab("pipeline")}
            className={`flex items-center gap-2 rounded-lg px-3.5 py-1.5 text-xs font-medium transition-all ${
              activeTab === "pipeline"
                ? "bg-white text-emerald-600 shadow-sm dark:bg-slate-700 dark:text-emerald-400"
                : "text-slate-600 hover:text-slate-900 dark:text-slate-300 dark:hover:text-white"
            }`}
          >
            <GitPullRequest className="h-3.5 w-3.5" />
            {t("prPipelineTab", "Issues 流水线")}
            {tasksCount > 0 && (
              <span className="ml-1 rounded-full bg-slate-200 dark:bg-slate-600 px-1.5 py-0.2 text-[10px] font-semibold text-slate-700 dark:text-slate-200">
                {tasksCount}
              </span>
            )}
          </button>

          <button
            type="button"
            onClick={() => setActiveTab("gateway")}
            className={`flex items-center gap-2 rounded-lg px-3.5 py-1.5 text-xs font-medium transition-all ${
              activeTab === "gateway"
                ? "bg-white text-emerald-600 shadow-sm dark:bg-slate-700 dark:text-emerald-400"
                : "text-slate-600 hover:text-slate-900 dark:text-slate-300 dark:hover:text-white"
            }`}
          >
            <Network className="h-3.5 w-3.5" />
            {t("cloudGatewayTab", "网关同步")}
          </button>
        </div>
      </div>

      {/* 次级状态与控制横条 */}
      <div className="px-6 py-2.5 bg-slate-50/60 dark:bg-slate-950/40 border-t border-slate-200/50 dark:border-slate-800/50 flex flex-wrap items-center justify-between gap-4 text-xs">
        {/* 左侧：主机切换与状态指示 */}
        <div className="flex flex-wrap items-center gap-4">
          <div className="flex items-center gap-2">
            <Server className="h-3.5 w-3.5 text-slate-400" />
            <div className="w-40">
              <CustomSelect
                size="sm"
                value={selectedHost}
                onChange={setSelectedHost}
                options={
                  hosts.length > 0
                    ? hosts.map((h) => ({ value: h.id, label: h.label }))
                    : [{ value: selectedHost, label: selectedHost }]
                }
              />
            </div>
          </div>

          {/* 守护服务状态 */}
          <div className="flex items-center gap-1.5 text-slate-600 dark:text-slate-300">
            <span
              className={`h-2 w-2 rounded-full ${
                status?.service_active
                  ? "bg-emerald-500 ring-2 ring-emerald-500/20"
                  : "bg-rose-500 ring-2 ring-rose-500/20"
              }`}
            />
            <span>
              {status?.service_active
                ? t("daemonActive", "守护进程运行中")
                : t("daemonInactive", "守护进程离线")}
            </span>
          </div>

          {/* 磁盘空间 */}
          {disk && (
            <div className="flex items-center gap-2">
              <HardDrive className={`h-3.5 w-3.5 ${isDiskAlert ? "text-amber-500" : "text-slate-400"}`} />
              <span className={isDiskAlert ? "text-amber-600 font-semibold" : "text-slate-500 dark:text-slate-400"}>
                {disk.used_gb}G / {disk.total_gb}G ({disk.use_percent}%)
              </span>
              <div className="w-16 h-1.5 rounded-full bg-slate-200 dark:bg-slate-700 overflow-hidden">
                <div
                  className={`h-full rounded-full ${
                    isDiskAlert ? "bg-amber-500" : "bg-emerald-500"
                  }`}
                  style={{ width: `${Math.min(100, disk.use_percent)}%` }}
                />
              </div>
            </div>
          )}
        </div>

        {/* 右侧：全局快捷动作 */}
        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={onOpenNewTaskModal}
            className="inline-flex items-center gap-1.5 rounded-lg bg-emerald-600 px-3 py-1.5 text-xs font-medium text-white shadow-sm hover:bg-emerald-700 transition-all"
          >
            <Sparkles className="h-3 w-3" />
            {t("newFeatureTaskBtn", "新建特性任务")}
          </button>

          <button
            type="button"
            onClick={onSyncGateway}
            className="inline-flex items-center gap-1.5 rounded-lg border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800 px-3 py-1.5 text-xs font-medium text-slate-700 dark:text-slate-200 hover:bg-slate-50 dark:hover:bg-slate-700/60 transition-all"
          >
            <Network className="h-3 w-3 text-emerald-500" />
            {t("syncGatewayBtn", "同步网关")}
          </button>

          <button
            type="button"
            disabled={loading}
            onClick={onRefresh}
            title={t("refresh", "刷新")}
            className="flex h-7 w-7 items-center justify-center rounded-lg border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800 text-slate-500 hover:text-slate-800 dark:hover:text-white transition-all disabled:opacity-50"
          >
            <RefreshCw className={`h-3.5 w-3.5 ${loading ? "animate-spin" : ""}`} />
          </button>

          <button
            type="button"
            onClick={() => {
              const win = window as any;
              win.setSettingsTab?.("cloud-agent");
              win.setSettingsOpen?.(true);
            }}
            title={t("settings", "设置")}
            className="flex h-7 w-7 items-center justify-center rounded-lg border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800 text-slate-500 hover:text-slate-800 dark:hover:text-white transition-all"
          >
            <Settings className="h-3.5 w-3.5" />
          </button>
        </div>
      </div>
    </div>
  );
}
