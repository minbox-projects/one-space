import { useCallback, useEffect, useMemo, useState } from "react";
import { Activity, Plus, RefreshCw, Search } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { ToolShell } from "@/components/toolbox/ToolShell";
import { useToast } from "@/components/ToastProvider";
import { isToolboxInvokeAvailable } from "@/toolbox/invoke";
import { MonitorCard } from "./MonitorCard";
import { MonitorFormModal } from "./MonitorFormModal";
import type { MonitorSummary, MonitorTarget, ProbeTestResult } from "./types";

export function UptimeMonitorTool() {
  const { pushToast } = useToast();
  const [summaries, setSummaries] = useState<MonitorSummary[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // 筛选与搜索
  const [searchQuery, setSearchQuery] = useState("");
  const [statusFilter, setStatusFilter] = useState<"all" | "up" | "down" | "paused">("all");

  // 弹窗状态
  const [modalOpen, setModalOpen] = useState(false);
  const [editingTarget, setEditingTarget] = useState<MonitorTarget | null>(null);
  const [testingId, setTestingId] = useState<string | null>(null);

  const fetchMonitors = useCallback(async () => {
    if (!isToolboxInvokeAvailable()) return;
    setLoading(true);
    try {
      const data = await invoke<MonitorSummary[]>("uptime_list_monitors");
      setSummaries(Array.isArray(data) ? data : []);
      setError(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void fetchMonitors();
    // 前台定时刷新视图
    const timer = setInterval(() => {
      void fetchMonitors();
    }, 15000);
    return () => clearInterval(timer);
  }, [fetchMonitors]);

  const handleToggle = async (id: string, enabled: boolean) => {
    try {
      await invoke("uptime_toggle_monitor", { id, enabled });
      setSummaries((prev) =>
        prev.map((s) =>
          s.target.id === id
            ? {
                ...s,
                target: { ...s.target, isEnabled: enabled },
                currentStatus: enabled ? s.currentStatus : "paused",
              }
            : s,
        ),
      );
      pushToast({
        title: enabled ? "已启用监控" : "已暂停监控",
        kind: "success",
      });
    } catch (err) {
      pushToast({
        title: "操作失败",
        description: String(err),
        kind: "error",
      });
    }
  };

  const handleTestProbe = async (summary: MonitorSummary) => {
    setTestingId(summary.target.id);
    try {
      const res = await invoke<ProbeTestResult>("uptime_test_probe", {
        target: summary.target,
      });
      pushToast({
        title: res.success ? "探针检测通过" : "探针检测失败",
        description: `${res.message} (${res.responseTimeMs}ms)`,
        kind: res.success ? "success" : "error",
      });
      void fetchMonitors();
    } catch (err) {
      pushToast({
        title: "检测发生异常",
        description: String(err),
        kind: "error",
      });
    } finally {
      setTestingId(null);
    }
  };

  const handleSave = async (target: MonitorTarget) => {
    try {
      await invoke("uptime_save_monitor", { target });
      pushToast({
        title: "保存成功",
        description: `监控项 [${target.name}] 已生效`,
        kind: "success",
      });
      void fetchMonitors();
    } catch (err) {
      pushToast({
        title: "保存失败",
        description: String(err),
        kind: "error",
      });
      throw err;
    }
  };

  const handleDelete = async (id: string) => {
    if (!window.confirm("确定要删除此监控项及其历史数据吗？")) return;
    try {
      await invoke("uptime_delete_monitor", { id });
      setSummaries((prev) => prev.filter((s) => s.target.id !== id));
      pushToast({
        title: "已删除监控项",
        kind: "success",
      });
    } catch (err) {
      pushToast({
        title: "删除失败",
        description: String(err),
        kind: "error",
      });
    }
  };

  const handleClearHistory = async (id: string) => {
    try {
      await invoke("uptime_clear_history", { id });
      pushToast({
        title: "历史心跳已重置",
        kind: "success",
      });
      void fetchMonitors();
    } catch (err) {
      pushToast({
        title: "重置失败",
        description: String(err),
        kind: "error",
      });
    }
  };

  const handleModalTestProbe = async (target: MonitorTarget): Promise<ProbeTestResult> => {
    return await invoke<ProbeTestResult>("uptime_test_probe", { target });
  };

  // 统计指标
  const safeSummaries = Array.isArray(summaries) ? summaries : [];
  const totalCount = safeSummaries.length;
  const upCount = safeSummaries.filter((s) => s.currentStatus === "up" || s.currentStatus === "degraded").length;
  const downCount = safeSummaries.filter((s) => s.currentStatus === "down").length;

  const filteredSummaries = useMemo(() => {
    return safeSummaries.filter((s) => {
      if (statusFilter === "up" && s.currentStatus !== "up" && s.currentStatus !== "degraded") {
        return false;
      }
      if (statusFilter === "down" && s.currentStatus !== "down") {
        return false;
      }
      if (statusFilter === "paused" && s.currentStatus !== "paused") {
        return false;
      }
      if (searchQuery.trim()) {
        const q = searchQuery.toLowerCase();
        const matchName = s.target.name.toLowerCase().includes(q);
        const matchTarget = s.target.target.toLowerCase().includes(q);
        const matchTags = s.target.tags.some((t) => t.toLowerCase().includes(q));
        return matchName || matchTarget || matchTags;
      }
      return true;
    });
  }, [summaries, statusFilter, searchQuery]);

  return (
    <ToolShell
      title="服务状态监控"
      description="本地与远程服务心跳探活、TCP端口检测、延迟趋势分析与可用性告警"
      icon={
        <div className="flex h-10 w-10 items-center justify-center rounded-xl bg-emerald-500/10 text-emerald-600">
          <Activity className="h-5 w-5" />
        </div>
      }
      error={error}
      actions={
        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={() => void fetchMonitors()}
            disabled={loading}
            className="inline-flex items-center gap-1.5 rounded-lg border border-border bg-card px-3 py-1.5 text-xs font-medium text-foreground hover:bg-accent transition-colors disabled:opacity-50"
            title="刷新状态"
          >
            <RefreshCw className={`w-3.5 h-3.5 ${loading ? "animate-spin" : ""}`} />
            刷新
          </button>
          <button
            type="button"
            onClick={() => {
              setEditingTarget(null);
              setModalOpen(true);
            }}
            className="inline-flex items-center gap-1.5 rounded-lg bg-primary px-3 py-1.5 text-xs font-medium text-primary-foreground hover:bg-primary/90 transition-colors shadow-sm"
          >
            <Plus className="w-3.5 h-3.5" />
            新建监控
          </button>
        </div>
      }
    >
      <div className="space-y-4">
        {/* 顶部概览仪表统计 */}
        <div className="grid grid-cols-2 sm:grid-cols-4 gap-3">
          <div className="rounded-xl border border-border/70 bg-card p-3 shadow-sm">
            <span className="text-xs text-muted-foreground block">监控总数</span>
            <span className="text-xl font-bold font-mono text-foreground mt-0.5 block">
              {totalCount}
            </span>
          </div>
          <div className="rounded-xl border border-border/70 bg-card p-3 shadow-sm">
            <span className="text-xs text-emerald-600 dark:text-emerald-400 block font-medium">
              正常运行
            </span>
            <span className="text-xl font-bold font-mono text-emerald-600 dark:text-emerald-400 mt-0.5 block">
              {upCount}
            </span>
          </div>
          <div className="rounded-xl border border-border/70 bg-card p-3 shadow-sm">
            <span className="text-xs text-rose-600 dark:text-rose-400 block font-medium">
              异常/故障
            </span>
            <span className="text-xl font-bold font-mono text-rose-600 dark:text-rose-400 mt-0.5 block">
              {downCount}
            </span>
          </div>
          <div className="rounded-xl border border-border/70 bg-card p-3 shadow-sm">
            <span className="text-xs text-muted-foreground block">状态健康度</span>
            <span className="text-xl font-bold font-mono text-foreground mt-0.5 block">
              {totalCount > 0 ? `${((upCount / totalCount) * 100).toFixed(0)}%` : "100%"}
            </span>
          </div>
        </div>

        {/* 筛选与搜索工具条 */}
        <div className="flex items-center justify-between gap-3 flex-wrap">
          <div className="flex items-center gap-1.5">
            <button
              type="button"
              onClick={() => setStatusFilter("all")}
              className={`rounded-lg px-2.5 py-1 text-xs font-medium transition-colors ${
                statusFilter === "all"
                  ? "bg-primary text-primary-foreground shadow-sm"
                  : "bg-muted/50 text-muted-foreground hover:bg-muted"
              }`}
            >
              全部 ({totalCount})
            </button>
            <button
              type="button"
              onClick={() => setStatusFilter("up")}
              className={`rounded-lg px-2.5 py-1 text-xs font-medium transition-colors ${
                statusFilter === "up"
                  ? "bg-emerald-600 text-white shadow-sm"
                  : "bg-muted/50 text-muted-foreground hover:bg-muted"
              }`}
            >
              正常 ({upCount})
            </button>
            <button
              type="button"
              onClick={() => setStatusFilter("down")}
              className={`rounded-lg px-2.5 py-1 text-xs font-medium transition-colors ${
                statusFilter === "down"
                  ? "bg-rose-600 text-white shadow-sm"
                  : "bg-muted/50 text-muted-foreground hover:bg-muted"
              }`}
            >
              异常 ({downCount})
            </button>
            <button
              type="button"
              onClick={() => setStatusFilter("paused")}
              className={`rounded-lg px-2.5 py-1 text-xs font-medium transition-colors ${
                statusFilter === "paused"
                  ? "bg-neutral-600 text-white shadow-sm"
                  : "bg-muted/50 text-muted-foreground hover:bg-muted"
              }`}
            >
              已暂停 ({summaries.filter((s) => s.currentStatus === "paused").length})
            </button>
          </div>

          <div className="relative min-w-[200px]">
            <Search className="w-3.5 h-3.5 absolute left-2.5 top-1/2 -translate-y-1/2 text-muted-foreground pointer-events-none" />
            <input
              type="text"
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              placeholder="搜索名称、地址、标签..."
              className="w-full rounded-lg border border-input bg-background pl-8 pr-3 py-1 text-xs text-foreground focus:outline-none focus:ring-1 focus:ring-ring"
            />
          </div>
        </div>

        {/* 监控项列表 */}
        {filteredSummaries.length > 0 ? (
          <div className="grid grid-cols-1 gap-3">
            {filteredSummaries.map((summary) => (
              <MonitorCard
                key={summary.target.id}
                summary={summary}
                onToggle={handleToggle}
                onTestProbe={handleTestProbe}
                onEdit={(s) => {
                  setEditingTarget(s.target);
                  setModalOpen(true);
                }}
                onDelete={handleDelete}
                onClearHistory={handleClearHistory}
                isTesting={testingId === summary.target.id}
              />
            ))}
          </div>
        ) : (
          <div className="rounded-xl border border-dashed border-border p-12 text-center">
            <Activity className="w-8 h-8 mx-auto text-muted-foreground/50 mb-3" />
            <h4 className="font-semibold text-sm text-foreground">暂无监控项</h4>
            <p className="text-xs text-muted-foreground mt-1 max-w-sm mx-auto">
              添加您的第一个 Web 网站、API 接口、内网数据库或服务器探活目标。
            </p>
            <button
              type="button"
              onClick={() => {
                setEditingTarget(null);
                setModalOpen(true);
              }}
              className="mt-4 inline-flex items-center gap-1.5 rounded-lg bg-primary px-3 py-1.5 text-xs font-medium text-primary-foreground hover:bg-primary/90 transition-colors"
            >
              <Plus className="w-3.5 h-3.5" />
              添加监控项
            </button>
          </div>
        )}
      </div>

      <MonitorFormModal
        isOpen={modalOpen}
        initialTarget={editingTarget}
        onClose={() => setModalOpen(false)}
        onSave={handleSave}
        onTestProbe={handleModalTestProbe}
      />
    </ToolShell>
  );
}
