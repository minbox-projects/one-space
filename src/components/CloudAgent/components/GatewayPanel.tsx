import { useState } from "react";
import { Network, CheckCircle2, AlertTriangle, Layers, ShieldCheck, Loader2, Sparkles } from "lucide-react";
import type { TFunction } from "i18next";
import type { CloudAgentStatus, GatewaySyncResult } from "@/lib/cloudAgent";

interface GatewayPanelProps {
  t: TFunction;
  status: CloudAgentStatus | null;
  loading?: boolean;
  onSyncGateway: () => Promise<GatewaySyncResult | undefined>;
}

export function GatewayPanel({
  t,
  status,
  onSyncGateway,
}: GatewayPanelProps) {
  const [isSyncing, setIsSyncing] = useState(false);
  const [syncResult, setSyncResult] = useState<GatewaySyncResult | null>(null);

  const handleSync = async () => {
    try {
      setIsSyncing(true);
      const res = await onSyncGateway();
      if (res) {
        setSyncResult(res);
      }
    } finally {
      setIsSyncing(false);
    }
  };

  const isHealthy =
    status?.gateway?.status === "ok" || status?.gateway?.status === "healthy";
  const modelPools = status?.gateway?.model_pools || [];

  return (
    <div className="space-y-6 max-w-4xl">
      {/* 网关状态总览卡片 */}
      <div className="rounded-2xl border border-slate-200 bg-white p-6 shadow-sm dark:border-slate-800 dark:bg-slate-900">
        <div className="flex flex-col md:flex-row md:items-center justify-between gap-4">
          <div className="flex items-center gap-3.5">
            <div className={`flex h-12 w-12 items-center justify-center rounded-2xl ${
              isHealthy
                ? "bg-emerald-500/10 text-emerald-600 dark:bg-emerald-500/20 dark:text-emerald-400"
                : "bg-amber-500/10 text-amber-600 dark:bg-amber-500/20 dark:text-amber-400"
            }`}>
              <Network className="h-6 w-6" />
            </div>
            <div>
              <div className="flex items-center gap-2">
                <h3 className="text-base font-semibold text-slate-900 dark:text-white">
                  {t("cloudGatewayTitle", "Linux AI 网关 (Port 17688)")}
                </h3>
                {isHealthy ? (
                  <span className="inline-flex items-center gap-1 rounded-full bg-emerald-500/10 px-2 py-0.5 text-[10px] font-semibold text-emerald-600 dark:bg-emerald-500/20 dark:text-emerald-400">
                    <CheckCircle2 className="h-3 w-3" />
                    {t("healthy", "健康运行中")}
                  </span>
                ) : (
                  <span className="inline-flex items-center gap-1 rounded-full bg-amber-500/10 px-2 py-0.5 text-[10px] font-semibold text-amber-600 dark:bg-amber-500/20 dark:text-amber-400">
                    <AlertTriangle className="h-3 w-3" />
                    {t("offlineOrStarting", "离线或启动中")}
                  </span>
                )}
              </div>
              <p className="text-xs text-slate-500 dark:text-slate-400 mt-0.5">
                {t(
                  "cloudGatewayDesc",
                  "轻量纯 Python OpenAI 兼容代理，支持多 Provider 轮询、Key 自动冷却与虚拟模型池。"
                )}
              </p>
            </div>
          </div>

          <button
            type="button"
            disabled={isSyncing}
            onClick={handleSync}
            className="inline-flex items-center gap-2 rounded-xl bg-emerald-600 px-4 py-2 text-xs font-medium text-white shadow-sm hover:bg-emerald-700 transition-all disabled:opacity-60 shrink-0"
          >
            {isSyncing ? (
              <>
                <Loader2 className="h-3.5 w-3.5 animate-spin" />
                {t("syncingGateway", "正在同步网关...")}
              </>
            ) : (
              <>
                <Sparkles className="h-3.5 w-3.5" />
                {t("syncGatewayToCloudBtn", "一键同步网关至云端")}
              </>
            )}
          </button>
        </div>

        {syncResult && (
          <div className="mt-4 rounded-xl bg-emerald-50 dark:bg-emerald-950/30 p-3.5 text-xs text-emerald-800 dark:text-emerald-300 flex items-center gap-2">
            <CheckCircle2 className="h-4 w-4 shrink-0 text-emerald-600 dark:text-emerald-400" />
            <span>{syncResult.message}</span>
          </div>
        )}
      </div>

      {/* 虚拟模型池 */}
      <div className="rounded-2xl border border-slate-200 bg-white p-6 shadow-sm dark:border-slate-800 dark:bg-slate-900 space-y-4">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <Layers className="h-4 w-4 text-emerald-500" />
            <h4 className="text-sm font-semibold text-slate-900 dark:text-white">
              {t("activeModelPools", "云端当前生效的模型池")}
            </h4>
          </div>
          <span className="text-xs text-slate-400">
            {modelPools.length} 个模型池
          </span>
        </div>

        {modelPools.length === 0 ? (
          <p className="text-xs text-slate-400 py-3">
            {t("noModelPoolsSynced", "云端暂无模型池，请点击右上角「一键同步网关至云端」完成配置下发。")}
          </p>
        ) : (
          <div className="grid grid-cols-1 sm:grid-cols-2 md:grid-cols-3 gap-3">
            {modelPools.map((poolName) => (
              <div
                key={poolName}
                className="rounded-xl border border-slate-100 bg-slate-50/80 p-3 text-xs dark:border-slate-800 dark:bg-slate-800/40"
              >
                <div className="font-mono font-medium text-slate-800 dark:text-slate-200">
                  {poolName}
                </div>
                <div className="text-[11px] text-slate-400 mt-1">
                  Virtual Model Pool
                </div>
              </div>
            ))}
          </div>
        )}
      </div>

      {/* 原理说明卡片 */}
      <div className="rounded-2xl border border-dashed border-slate-200 bg-slate-50/60 p-5 dark:border-slate-800 dark:bg-slate-900/30 text-xs text-slate-500 dark:text-slate-400 space-y-2">
        <div className="font-semibold text-slate-700 dark:text-slate-300 flex items-center gap-1.5">
          <ShieldCheck className="h-4 w-4 text-emerald-500" />
          {t("cloudSyncArchitectureTitle", "网关安全同步机制与双端协同")}
        </div>
        <p>
          1. <strong>安全免密通道</strong>：同步通过本地 Tauri 后端建立的临时安全 SSH 通道写入，无需在云端开放公网管理端口。
        </p>
        <p>
          2. <strong>智能配置合流</strong>：仅覆盖云端的 <code>ai_gateway</code> 节，云端保留现有的 GitHub Token、监听仓库及通知 Webhook 配置。
        </p>
        <p>
          3. <strong>OpenCode 自动对齐</strong>：重载后，云端 OpenCode 自动将本地配置的模型映射为 <code>Cloud Agent Gateway</code> 提供商，立享一致的调用额度。
        </p>
      </div>
    </div>
  );
}
