import { useEffect, useState } from "react";
import {
  Activity,
  Check,
  Copy,
  Globe,
  Loader2,
  ShieldAlert,
  ShieldCheck,
  X,
  Zap,
} from "lucide-react";
import { useCopyToClipboard } from "@/toolbox/useCopyToClipboard";
import {
  dnsIpLookup,
  dnsTcpPing,
  type IpIntelligenceResult,
  type TcpPingResult,
} from "@/lib/dnsInspector";

interface IpDetailDrawerProps {
  ip: string | null;
  onClose: () => void;
  onProbeWithPingPe?: (ip: string, port: number) => void;
}

export function IpDetailDrawer({
  ip,
  onClose,
  onProbeWithPingPe,
}: IpDetailDrawerProps) {
  const { copy, copied } = useCopyToClipboard();
  const [intel, setIntel] = useState<IpIntelligenceResult | null>(null);
  const [loadingIntel, setLoadingIntel] = useState(false);

  // TCP Ping 状态
  const [pingPort, setPingPort] = useState(80);
  const [pinging, setPinging] = useState(false);
  const [pingResult, setPingResult] = useState<TcpPingResult | null>(null);

  useEffect(() => {
    if (!ip) {
      setIntel(null);
      setPingResult(null);
      return;
    }

    let active = true;
    setLoadingIntel(true);

    dnsIpLookup(ip)
      .then((res) => {
        if (active) setIntel(res);
      })
      .catch(() => {
        if (active) setIntel(null);
      })
      .finally(() => {
        if (active) setLoadingIntel(false);
      });

    return () => {
      active = false;
    };
  }, [ip]);

  if (!ip) return null;

  const handleRunPing = async () => {
    setPinging(true);
    setPingResult(null);
    try {
      const res = await dnsTcpPing(ip, pingPort, 4000);
      setPingResult(res);
    } catch (err) {
      setPingResult({
        host: ip,
        port: pingPort,
        success: false,
        error: err instanceof Error ? err.message : String(err),
      });
    } finally {
      setPinging(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex justify-end bg-black/40 backdrop-blur-xs transition-opacity animate-in fade-in">
      <div className="flex h-full w-full max-w-md flex-col border-l border-slate-200/80 bg-white p-6 shadow-2xl dark:border-slate-800 dark:bg-slate-900">
        {/* 抽屉头部 */}
        <div className="flex items-center justify-between border-b border-slate-100 pb-4 dark:border-slate-800">
          <div className="flex items-center gap-2">
            <Globe className="h-4 w-4 text-indigo-500" />
            <h2 className="text-sm font-semibold text-slate-800 dark:text-slate-100">
              IP 网络情报与握手探测
            </h2>
          </div>
          <button
            type="button"
            onClick={onClose}
            className="rounded-lg p-1.5 text-slate-400 hover:bg-slate-100 hover:text-slate-700 dark:hover:bg-slate-800 dark:hover:text-slate-200"
          >
            <X className="h-4 w-4" />
          </button>
        </div>

        {/* 抽屉内容区 */}
        <div className="flex-1 space-y-6 overflow-y-auto py-5 text-xs">
          {/* 目标 IP 卡片 */}
          <div className="rounded-xl border border-slate-200/80 bg-slate-50/60 p-4 dark:border-slate-800 dark:bg-slate-800/50">
            <span className="text-[11px] font-medium text-slate-400">目标地址</span>
            <div className="mt-1 flex items-center justify-between">
              <span className="font-mono text-base font-bold text-slate-800 dark:text-slate-100">
                {ip}
              </span>
              <button
                type="button"
                onClick={() => copy(ip)}
                className="flex items-center gap-1 rounded-md border border-slate-200 bg-white px-2 py-1 text-slate-700 hover:bg-slate-50 dark:border-slate-700 dark:bg-slate-800 dark:text-slate-200"
              >
                {copied ? <Check className="h-3 w-3 text-emerald-500" /> : <Copy className="h-3 w-3" />}
                <span>{copied ? "已复制" : "复制"}</span>
              </button>
            </div>
          </div>

          {/* IP 属性与分类 */}
          <div className="rounded-xl border border-slate-200/80 bg-white p-4 dark:border-slate-800 dark:bg-slate-800/40">
            <h3 className="mb-3 font-semibold text-slate-700 dark:text-slate-300">
              网络属性分类
            </h3>

            {loadingIntel ? (
              <div className="flex items-center gap-2 text-slate-400">
                <Loader2 className="h-4 w-4 animate-spin text-indigo-500" />
                <span>分析 IP 属性中...</span>
              </div>
            ) : intel ? (
              <div className="space-y-2.5">
                <div className="flex items-center justify-between">
                  <span className="text-slate-400">地址类型:</span>
                  <span
                    className={`inline-flex items-center gap-1 rounded-md px-2 py-0.5 font-medium ${
                      intel.is_private
                        ? "bg-amber-500/10 text-amber-600 dark:text-amber-400"
                        : "bg-emerald-500/10 text-emerald-600 dark:text-emerald-400"
                    }`}
                  >
                    {intel.is_private ? (
                      <ShieldAlert className="h-3 w-3" />
                    ) : (
                      <ShieldCheck className="h-3 w-3" />
                    )}
                    <span>{intel.classification}</span>
                  </span>
                </div>
              </div>
            ) : null}
          </div>

          {/* TCP 端口握手连通性探测 */}
          <div className="rounded-xl border border-slate-200/80 bg-white p-4 dark:border-slate-800 dark:bg-slate-800/40">
            <h3 className="mb-2 font-semibold text-slate-700 dark:text-slate-300">
              TCP 端口握手时延测试
            </h3>
            <p className="mb-3 text-[11px] text-slate-400">
              通过本机发起真实 TCP 三次握手，测量与目标主机的网络往返时延 (RTT)
            </p>

            <div className="flex items-center gap-2">
              <span className="text-slate-500">目标端口:</span>
              <div className="flex gap-1.5">
                {[80, 443, 8080, 22].map((p) => (
                  <button
                    key={p}
                    type="button"
                    onClick={() => setPingPort(p)}
                    className={`rounded-md px-2 py-1 font-mono text-[11px] font-medium ${
                      pingPort === p
                        ? "bg-indigo-600 text-white"
                        : "border border-slate-200 bg-slate-50 text-slate-600 hover:bg-slate-100 dark:border-slate-700 dark:bg-slate-800 dark:text-slate-300"
                    }`}
                  >
                    {p}
                  </button>
                ))}
              </div>

              <button
                type="button"
                onClick={handleRunPing}
                disabled={pinging}
                className="ml-auto flex items-center gap-1 rounded-lg bg-indigo-600 px-3 py-1.5 font-semibold text-white hover:bg-indigo-500 disabled:opacity-40"
              >
                {pinging ? (
                  <Loader2 className="h-3.5 w-3.5 animate-spin" />
                ) : (
                  <Activity className="h-3.5 w-3.5" />
                )}
                <span>{pinging ? "探测中..." : "测试握手"}</span>
              </button>
            </div>

            {/* 探测结果 */}
            {pingResult ? (
              <div
                className={`mt-3 rounded-lg border p-3 ${
                  pingResult.success
                    ? "border-emerald-500/30 bg-emerald-500/10 text-emerald-800 dark:text-emerald-300"
                    : "border-rose-500/30 bg-rose-500/10 text-rose-800 dark:text-rose-300"
                }`}
              >
                <div className="flex items-center justify-between">
                  <span className="font-semibold">
                    {pingResult.success ? "握手建立成功" : "握手失败"}
                  </span>
                  {pingResult.latency_ms !== null && pingResult.latency_ms !== undefined && (
                    <span className="font-mono text-sm font-bold">
                      {pingResult.latency_ms} ms
                    </span>
                  )}
                </div>
                {pingResult.error ? (
                  <p className="mt-1 font-mono text-[11px] text-rose-600 dark:text-rose-400">
                    {pingResult.error}
                  </p>
                ) : null}
              </div>
            ) : null}
          </div>
        </div>

        {/* 底部操作区 */}
        <div className="space-y-2 border-t border-slate-100 pt-4 dark:border-slate-800">
          {onProbeWithPingPe && (
            <button
              type="button"
              onClick={() => {
                onProbeWithPingPe(ip, pingPort);
                onClose();
              }}
              className="flex w-full items-center justify-center gap-1.5 rounded-xl border border-indigo-200 bg-indigo-50/70 py-2 text-xs font-semibold text-indigo-700 transition-colors hover:bg-indigo-100 dark:border-indigo-800 dark:bg-indigo-950/40 dark:text-indigo-300 dark:hover:bg-indigo-900/50"
            >
              <Zap className="h-3.5 w-3.5" />
              <span>带入 tcp.ping.pe 启动持续心跳检测</span>
            </button>
          )}

          <button
            type="button"
            onClick={onClose}
            className="w-full rounded-xl bg-slate-100 py-2 text-xs font-semibold text-slate-700 hover:bg-slate-200 dark:bg-slate-800 dark:text-slate-300 dark:hover:bg-slate-700"
          >
            关闭
          </button>
        </div>
      </div>
    </div>
  );
}
