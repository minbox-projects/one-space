import { CheckCircle2, Clock, Globe2, ShieldCheck, XCircle } from "lucide-react";
import type { DnsCompareReport, DnsRecordAnswer } from "@/lib/dnsInspector";

interface DnsComparisonGridProps {
  report: DnsCompareReport | null;
  loading: boolean;
  onSelectIp: (ip: string) => void;
}

export function DnsComparisonGrid({
  report,
  loading,
  onSelectIp,
}: DnsComparisonGridProps) {
  if (loading) {
    return (
      <div className="grid grid-cols-1 gap-4 md:grid-cols-2 lg:grid-cols-4">
        {[1, 2, 3, 4].map((i) => (
          <div
            key={i}
            className="h-64 animate-pulse rounded-2xl border border-slate-200/80 bg-white p-5 shadow-xs dark:border-slate-800 dark:bg-slate-900"
          >
            <div className="h-5 w-28 rounded bg-slate-100 dark:bg-slate-800" />
            <div className="mt-4 h-8 w-20 rounded bg-slate-100 dark:bg-slate-800" />
            <div className="mt-6 space-y-2">
              <div className="h-4 w-full rounded bg-slate-100 dark:bg-slate-800" />
              <div className="h-4 w-3/4 rounded bg-slate-100 dark:bg-slate-800" />
            </div>
          </div>
        ))}
      </div>
    );
  }

  if (!report) {
    return (
      <div className="flex h-64 flex-col items-center justify-center rounded-2xl border border-dashed border-slate-200 bg-white/50 text-center text-xs text-slate-400 dark:border-slate-800 dark:bg-slate-900/50">
        <Globe2 className="h-8 w-8 text-slate-300 dark:text-slate-600" />
        <p className="mt-2 font-medium">输入域名并点击「发起解析探测」</p>
        <p className="text-[11px]">将同时向各大公共 DNS 发起请求，横向比对解析生效与时延</p>
      </div>
    );
  }

  return (
    <div className="space-y-4">
      {/* 一致性指示器条 */}
      <div
        className={`flex items-center justify-between rounded-xl border px-4 py-2.5 text-xs font-medium shadow-xs ${
          report.is_consistent
            ? "border-emerald-500/30 bg-emerald-500/10 text-emerald-800 dark:text-emerald-300"
            : "border-amber-500/30 bg-amber-500/10 text-amber-800 dark:text-amber-300"
        }`}
      >
        <div className="flex items-center gap-2">
          {report.is_consistent ? (
            <ShieldCheck className="h-4 w-4 text-emerald-600 dark:text-emerald-400" />
          ) : (
            <CheckCircle2 className="h-4 w-4 text-amber-600 dark:text-amber-400" />
          )}
          <span>
            {report.is_consistent
              ? "多节点解析一致：各公共 DNS 解析结果数据集合完全吻合"
              : "检测到解析差异：不同 DNS 返回记录不一致（可能正在生效中或存在区域调度）"}
          </span>
        </div>
        <span className="font-mono text-[11px] text-slate-500">
          类型: {report.record_type}
        </span>
      </div>

      {/* 4 卡片网格 */}
      <div className="grid grid-cols-1 gap-4 md:grid-cols-2 lg:grid-cols-4">
        {report.servers.map((server) => {
          const isSuccess = server.error === null && server.status === 0;

          return (
            <div
              key={server.server_id}
              className="flex flex-col justify-between rounded-2xl border border-slate-200/80 bg-white p-5 shadow-xs transition-all hover:border-slate-300 dark:border-slate-800/80 dark:bg-slate-900 dark:hover:border-slate-700"
            >
              <div>
                {/* 卡片头部 */}
                <div className="flex items-start justify-between">
                  <div>
                    <h4 className="text-xs font-semibold text-slate-900 dark:text-white">
                      {server.server_name}
                    </h4>
                    <span className="mt-0.5 inline-block font-mono text-[10px] text-slate-400">
                      {server.server_id}
                    </span>
                  </div>

                  <span
                    className={`inline-flex items-center gap-1 rounded-full px-2 py-0.5 font-mono text-[10px] font-semibold ${
                      isSuccess
                        ? "bg-emerald-500/10 text-emerald-600 dark:bg-emerald-500/20 dark:text-emerald-400"
                        : "bg-rose-500/10 text-rose-600 dark:bg-rose-500/20 dark:text-rose-400"
                    }`}
                  >
                    {isSuccess ? <CheckCircle2 className="h-2.5 w-2.5" /> : <XCircle className="h-2.5 w-2.5" />}
                    <span>{server.status_text}</span>
                  </span>
                </div>

                {/* 耗时与指标 */}
                <div className="mt-3 flex items-center gap-1.5 text-xs text-slate-500">
                  <Clock className="h-3.5 w-3.5 text-slate-400" />
                  <span className="font-mono text-sm font-bold text-slate-800 dark:text-slate-100">
                    {server.latency_ms} ms
                  </span>
                  <span className="text-[11px] text-slate-400">响应耗时</span>
                </div>

                {/* 解析答案列表 */}
                <div className="mt-4 space-y-2 border-t border-slate-100 pt-3 dark:border-slate-800">
                  {server.error ? (
                    <p className="font-mono text-[11px] text-rose-500">
                      {server.error}
                    </p>
                  ) : server.answers.length === 0 ? (
                    <p className="text-[11px] text-slate-400 italic">
                      无对应记录 (空答案)
                    </p>
                  ) : (
                    server.answers.map((ans: DnsRecordAnswer, idx: number) => (
                      <div
                        key={idx}
                        className="group flex flex-col gap-0.5 rounded-lg border border-slate-100 bg-slate-50/60 p-2 text-xs transition-colors hover:border-indigo-200 hover:bg-indigo-50/30 dark:border-slate-800 dark:bg-slate-800/40 dark:hover:border-indigo-800"
                      >
                        <div className="flex items-center justify-between text-[10px] text-slate-400">
                          <span className="font-semibold text-indigo-600 dark:text-indigo-400">
                            {ans.type_name}
                          </span>
                          <span className="font-mono">TTL: {ans.TTL}s</span>
                        </div>

                        <button
                          type="button"
                          onClick={() => onSelectIp(ans.data)}
                          title="点击查看此 IP 深度情报与握手时延"
                          className="mt-0.5 break-all text-left font-mono font-medium text-slate-700 hover:text-indigo-600 dark:text-slate-200 dark:hover:text-indigo-400"
                        >
                          {ans.data}
                        </button>
                      </div>
                    ))
                  )}
                </div>
              </div>

              {/* 卡片底部提示 */}
              {isSuccess && server.answers.length > 0 && (
                <div className="mt-4 border-t border-slate-100 pt-2 text-[10px] text-slate-400 dark:border-slate-800">
                  共返回 {server.answers.length} 条记录
                </div>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}
