import { useState } from "react";
import { Activity, Globe, Loader2, Search, Sparkles } from "lucide-react";
import { useToast } from "@/components/ToastProvider";
import { dnsQuery, type DnsCompareReport } from "@/lib/dnsInspector";

import { DnsComparisonGrid } from "./DnsComparisonGrid";
import { IpDetailDrawer } from "./IpDetailDrawer";
import { TcpPingPePanel } from "./TcpPingPePanel";

const RECORD_TYPES = ["A", "AAAA", "CNAME", "TXT", "MX", "NS"];
const QUICK_DOMAINS = ["github.com", "cloudflare.com", "google.com", "taobao.com"];

type DnsInspectorTab = "pingpe" | "doh";

export function DnsInspectorTool() {
  const { pushToast } = useToast();

  const [activeTab, setActiveTab] = useState<DnsInspectorTab>("pingpe");
  const [targetHost, setTargetHost] = useState("1.1.1.1");
  const [targetPort, setTargetPort] = useState(443);

  // DoH 状态
  const [domain, setDomain] = useState("github.com");
  const [recordType, setRecordType] = useState("A");
  const [loading, setLoading] = useState(false);
  const [report, setReport] = useState<DnsCompareReport | null>(null);

  // 选中的 IP（用于详情抽屉）
  const [selectedIp, setSelectedIp] = useState<string | null>(null);

  const handleQuery = async (targetDomain = domain, targetType = recordType) => {
    const cleanDomain = targetDomain.trim();
    if (!cleanDomain) {
      pushToast({ title: "请输入要探测的域名", kind: "error" });
      return;
    }

    setLoading(true);
    try {
      const res = await dnsQuery(cleanDomain, targetType);
      setReport(res);
      pushToast({
        title: `已完成 ${cleanDomain} (${targetType}) 的全节点并发探测`,
        kind: "success",
      });
    } catch (err) {
      pushToast({
        title: "DNS 探测失败: " + (err instanceof Error ? err.message : String(err)),
        kind: "error",
      });
    } finally {
      setLoading(false);
    }
  };

  const handleSelectQuick = (d: string) => {
    setDomain(d);
    void handleQuery(d, recordType);
  };

  const handleProbeWithPingPe = (ip: string, port: number) => {
    setTargetHost(ip);
    setTargetPort(port);
    setActiveTab("pingpe");
    pushToast({
      title: `已将 IP ${ip}:${port} 带入 tcp.ping.pe 持续握手探测面板`,
      kind: "success",
    });
  };

  return (
    <div className="flex h-full flex-col bg-slate-50 text-slate-800 dark:bg-slate-950 dark:text-slate-100">
      {/* 顶部主工具栏 (Header) */}
      <div className="border-b border-slate-200/80 bg-white/70 px-6 py-4 backdrop-blur-md dark:border-slate-800/80 dark:bg-slate-900/70">
        <div className="flex flex-wrap items-center justify-between gap-4">
          <div className="flex items-center gap-3">
            <div className="flex h-10 w-10 items-center justify-center rounded-xl bg-indigo-500/10 text-indigo-600 shadow-xs dark:bg-indigo-500/20 dark:text-indigo-400">
              <Globe className="h-6 w-6" />
            </div>
            <div>
              <h1 className="text-lg font-semibold tracking-tight text-slate-900 dark:text-white">
                网络探测与 DNS 诊断
              </h1>
              <p className="text-xs text-slate-500 dark:text-slate-400">
                tcp.ping.pe 持续心跳方块图 · 丢包与抖动大盘 · 多公共 DNS 实时比对 · IP 归属分析
              </p>
            </div>
          </div>

          {/* 模式切换 Tabs */}
          <div className="flex rounded-xl bg-slate-100 p-1 dark:bg-slate-800">
            <button
              type="button"
              onClick={() => setActiveTab("pingpe")}
              className={`flex items-center gap-1.5 rounded-lg px-3.5 py-1.5 text-xs font-medium transition-all ${
                activeTab === "pingpe"
                  ? "bg-white text-indigo-600 shadow-xs dark:bg-slate-700 dark:text-indigo-400"
                  : "text-slate-600 hover:text-slate-900 dark:text-slate-300 dark:hover:text-white"
              }`}
            >
              <Activity className="h-3.5 w-3.5" />
              <span>网络探测 (tcp.ping.pe)</span>
            </button>
            <button
              type="button"
              onClick={() => setActiveTab("doh")}
              className={`flex items-center gap-1.5 rounded-lg px-3.5 py-1.5 text-xs font-medium transition-all ${
                activeTab === "doh"
                  ? "bg-white text-indigo-600 shadow-xs dark:bg-slate-700 dark:text-indigo-400"
                  : "text-slate-600 hover:text-slate-900 dark:text-slate-300 dark:hover:text-white"
              }`}
            >
              <Globe className="h-3.5 w-3.5" />
              <span>公共 DNS 比对 (DoH)</span>
            </button>
          </div>
        </div>

        {/* 仅在 DoH 模式下显示查询输入条与快捷标签 */}
        {activeTab === "doh" && (
          <div className="mt-4 flex flex-wrap items-center gap-3">
            {/* 域名输入框 */}
            <div className="relative min-w-72 flex-1">
              <Search className="absolute left-3 top-2.5 h-4 w-4 text-slate-400" />
              <input
                type="text"
                value={domain}
                onChange={(e) => setDomain(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") {
                    void handleQuery();
                  }
                }}
                placeholder="输入待查询域名 (如 example.com)"
                className="w-full rounded-xl border border-slate-200/80 bg-white py-2 pl-9 pr-3 font-mono text-xs text-slate-800 placeholder-slate-400 shadow-xs focus:border-indigo-500 focus:outline-none dark:border-slate-800 dark:bg-slate-900 dark:text-slate-100 dark:focus:border-indigo-400"
              />
            </div>

            {/* 记录类型 Pills */}
            <div className="flex rounded-xl bg-slate-100 p-0.5 dark:bg-slate-800">
              {RECORD_TYPES.map((t) => (
                <button
                  key={t}
                  type="button"
                  onClick={() => {
                    setRecordType(t);
                    if (domain.trim()) {
                      void handleQuery(domain, t);
                    }
                  }}
                  className={`rounded-lg px-2.5 py-1 text-xs font-semibold transition-all ${
                    recordType === t
                      ? "bg-white text-indigo-600 shadow-xs dark:bg-slate-700 dark:text-indigo-400"
                      : "text-slate-500 hover:text-slate-800 dark:text-slate-400 dark:hover:text-slate-200"
                  }`}
                >
                  {t}
                </button>
              ))}
            </div>

            {/* 发起探测按钮 */}
            <button
              type="button"
              onClick={() => void handleQuery()}
              disabled={loading || !domain.trim()}
              className="flex items-center gap-1.5 rounded-xl bg-indigo-600 px-4 py-2 text-xs font-semibold text-white shadow-xs transition-all hover:bg-indigo-500 disabled:opacity-40"
            >
              {loading ? (
                <Loader2 className="h-3.5 w-3.5 animate-spin" />
              ) : (
                <Globe className="h-3.5 w-3.5" />
              )}
              <span>{loading ? "全节点探测中..." : "发起解析探测"}</span>
            </button>

            {/* 快捷推荐 */}
            <div className="flex items-center gap-1.5">
              <Sparkles className="h-3.5 w-3.5 text-indigo-500" />
              <span className="text-[11px] font-medium text-slate-400">快捷:</span>
              {QUICK_DOMAINS.map((d) => (
                <button
                  key={d}
                  type="button"
                  onClick={() => handleSelectQuick(d)}
                  disabled={loading}
                  className="rounded-lg border border-slate-200/80 bg-white px-2 py-1 text-xs text-slate-600 hover:bg-slate-50 hover:text-indigo-600 disabled:opacity-40 dark:border-slate-800 dark:bg-slate-900 dark:text-slate-300 dark:hover:bg-slate-800"
                >
                  {d}
                </button>
              ))}
            </div>
          </div>
        )}
      </div>

      {/* 主工作区 */}
      <div className="flex-1 overflow-y-auto p-6">
        {activeTab === "pingpe" ? (
          <TcpPingPePanel initialHost={targetHost} initialPort={targetPort} />
        ) : (
          <DnsComparisonGrid
            report={report}
            loading={loading}
            onSelectIp={setSelectedIp}
          />
        )}
      </div>

      {/* IP 情报与测速抽屉 */}
      <IpDetailDrawer
        ip={selectedIp}
        onClose={() => setSelectedIp(null)}
        onProbeWithPingPe={handleProbeWithPingPe}
      />
    </div>
  );
}
