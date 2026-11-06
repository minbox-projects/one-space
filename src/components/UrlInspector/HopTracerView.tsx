import { useState } from "react";
import {
  AlertTriangle,
  ArrowDown,
  CheckCircle2,
  Clock,
  Compass,
  Copy,
  Cookie,
  ExternalLink,
  Layers,
  Loader2,
  Smartphone,
  Sparkles,
} from "lucide-react";
import { useToast } from "@/components/ToastProvider";
import { useCopyToClipboard } from "@/toolbox/useCopyToClipboard";
import { traceRedirectHops, type UrlHopDetail, type UrlTraceReport } from "@/lib/urlTracer";

const UA_PRESETS = [
  {
    label: "Desktop Chrome (macOS)",
    value:
      "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36",
  },
  {
    label: "iPhone Safari (iOS 18)",
    value:
      "Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Mobile/15E148 Safari/604.1",
  },
  {
    label: "微信内置浏览器 (WeChat WebView)",
    value:
      "Mozilla/5.0 (Linux; Android 14; Mobile) AppleWebKit/537.36 (KHTML, like Gecko) Version/4.0 Chrome/126.0.0.0 Mobile Safari/537.36 MicroMessenger/8.0.49",
  },
];

const DEMO_TRACE_URLS = [
  "https://t.co",
  "https://bit.ly",
  "https://github.com/microsoft/vscode",
];

export function HopTracerView() {
  const { pushToast } = useToast();
  const { copy, copied } = useCopyToClipboard();

  const [url, setUrl] = useState("https://github.com/microsoft/vscode");
  const [selectedUa, setSelectedUa] = useState(UA_PRESETS[0].value);
  const [loading, setLoading] = useState(false);
  const [report, setReport] = useState<UrlTraceReport | null>(null);

  const handleStartTrace = async (targetUrl = url) => {
    const clean = targetUrl.trim();
    if (!clean) {
      pushToast({ title: "请输入要追踪的起始网址", kind: "error" });
      return;
    }

    setLoading(true);
    try {
      const res = await traceRedirectHops(clean, selectedUa);
      setReport(res);
      if (res.error) {
        pushToast({ title: res.error, kind: "error" });
      } else {
        pushToast({
          title: `追踪完成，共检测到 ${res.total_hops} 级跳转`,
          kind: "success",
        });
      }
    } catch (err) {
      pushToast({
        title: "追踪失败: " + (err instanceof Error ? err.message : String(err)),
        kind: "error",
      });
    } finally {
      setLoading(false);
    }
  };

  const parseQueryParams = (rawUrl: string) => {
    try {
      const u = new URL(rawUrl);
      const params: Array<[string, string]> = [];
      u.searchParams.forEach((val, key) => {
        params.push([key, val]);
      });
      return params;
    } catch {
      return [];
    }
  };

  const getStatusBadge = (code: number) => {
    if (code >= 200 && code < 300) {
      return "border-emerald-500/30 bg-emerald-500/10 text-emerald-600 dark:text-emerald-400";
    }
    if (code >= 300 && code < 400) {
      return "border-amber-500/30 bg-amber-500/10 text-amber-600 dark:text-amber-400";
    }
    return "border-rose-500/30 bg-rose-500/10 text-rose-600 dark:text-rose-400";
  };

  return (
    <div className="space-y-5">
      {/* 搜索与选项控制卡片 */}
      <div className="rounded-2xl border border-slate-200/80 bg-white p-5 shadow-xs dark:border-slate-800/80 dark:bg-slate-900">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <label className="text-xs font-semibold text-slate-700 dark:text-slate-300">
            起始 URL 追踪地址（自动逐级拦截 301/302/307 重定向）
          </label>
          <div className="flex items-center gap-1.5 text-xs text-slate-400">
            <Sparkles className="h-3.5 w-3.5 text-violet-500" />
            <span>快速载入:</span>
            {DEMO_TRACE_URLS.map((demo) => (
              <button
                key={demo}
                type="button"
                onClick={() => {
                  setUrl(demo);
                  void handleStartTrace(demo);
                }}
                disabled={loading}
                className="rounded-md border border-slate-200/80 bg-slate-50 px-2 py-0.5 font-mono text-[11px] text-slate-600 hover:bg-slate-100 dark:border-slate-800 dark:bg-slate-800 dark:text-slate-300"
              >
                {demo}
              </button>
            ))}
          </div>
        </div>

        <div className="mt-3 flex flex-wrap items-center gap-3">
          {/* 输入框 */}
          <div className="relative min-w-72 flex-1">
            <Compass className="absolute left-3 top-2.5 h-4 w-4 text-slate-400" />
            <input
              type="text"
              value={url}
              onChange={(e) => setUrl(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") void handleStartTrace();
              }}
              placeholder="在此粘贴任意待追踪的起始 URL..."
              className="w-full rounded-xl border border-slate-200/80 bg-slate-50/50 py-2 pl-9 pr-3 font-mono text-xs text-slate-800 placeholder-slate-400 focus:border-violet-500 focus:bg-white focus:outline-none dark:border-slate-800 dark:bg-slate-800/50 dark:text-slate-100 dark:focus:border-violet-400 dark:focus:bg-slate-800"
            />
          </div>

          {/* User-Agent 选择 */}
          <div className="flex items-center gap-1.5 rounded-xl border border-slate-200/80 bg-white px-3 py-1.5 text-xs text-slate-700 shadow-xs dark:border-slate-800 dark:bg-slate-900 dark:text-slate-300">
            <Smartphone className="h-3.5 w-3.5 text-slate-400" />
            <select
              value={selectedUa}
              onChange={(e) => setSelectedUa(e.target.value)}
              className="bg-transparent text-xs font-medium focus:outline-none dark:bg-slate-900"
            >
              {UA_PRESETS.map((ua) => (
                <option key={ua.label} value={ua.value}>
                  {ua.label}
                </option>
              ))}
            </select>
          </div>

          {/* 启动追踪按钮 */}
          <button
            type="button"
            onClick={() => void handleStartTrace()}
            disabled={loading || !url.trim()}
            className="flex items-center gap-1.5 rounded-xl bg-violet-600 px-4 py-2 text-xs font-semibold text-white shadow-xs transition-all hover:bg-violet-500 disabled:opacity-40"
          >
            {loading ? (
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
            ) : (
              <Compass className="h-3.5 w-3.5" />
            )}
            <span>{loading ? "逐级探测中..." : "开始追踪跳转"}</span>
          </button>
        </div>
      </div>

      {/* 追踪报告总览条 */}
      {report && (
        <div className="flex flex-wrap items-center justify-between gap-3 rounded-2xl border border-slate-200/80 bg-white p-4 text-xs shadow-xs dark:border-slate-800/80 dark:bg-slate-900">
          <div className="flex items-center gap-3">
            <div className="flex items-center gap-1.5 font-semibold text-slate-800 dark:text-slate-100">
              <Layers className="h-4 w-4 text-violet-500" />
              <span>总跳数: {report.total_hops} 级</span>
            </div>
            <div className="flex items-center gap-1.5 text-slate-500">
              <Clock className="h-4 w-4 text-slate-400" />
              <span>累计耗时: {report.total_duration_ms} ms</span>
            </div>
          </div>

          <div className="flex items-center gap-2">
            {report.has_loop ? (
              <span className="flex items-center gap-1 rounded-md bg-rose-500/10 px-2 py-0.5 font-semibold text-rose-600 dark:text-rose-400">
                <AlertTriangle className="h-3.5 w-3.5" />
                <span>检测到死循环环路</span>
              </span>
            ) : (
              <span className="flex items-center gap-1 rounded-md bg-emerald-500/10 px-2 py-0.5 font-semibold text-emerald-600 dark:text-emerald-400">
                <CheckCircle2 className="h-3.5 w-3.5" />
                <span>终态到达正常</span>
              </span>
            )}
          </div>
        </div>
      )}

      {/* 逐级跳转时间轴流水线 (Hop-by-Hop Pipeline) */}
      {report && (
        <div className="space-y-4">
          {report.hops.map((hop: UrlHopDetail, index: number) => {
            const queryParams = parseQueryParams(hop.url);

            return (
              <div key={hop.hop_index} className="relative">
                {/* 连线与向下的流动箭头 */}
                {index > 0 && (
                  <div className="flex items-center justify-center py-2">
                    <div className="flex h-6 w-6 items-center justify-center rounded-full bg-slate-100 text-slate-400 shadow-xs dark:bg-slate-800 dark:text-slate-500">
                      <ArrowDown className="h-3.5 w-3.5 animate-bounce" />
                    </div>
                  </div>
                )}

                <div className="rounded-2xl border border-slate-200/80 bg-white p-5 shadow-xs transition-all hover:border-slate-300 dark:border-slate-800/80 dark:bg-slate-900 dark:hover:border-slate-700">
                  {/* Hop 卡片 Header */}
                  <div className="flex flex-wrap items-center justify-between gap-3 border-b border-slate-100 pb-3 dark:border-slate-800">
                    <div className="flex items-center gap-2">
                      <span className="flex h-6 w-6 items-center justify-center rounded-lg bg-violet-500/10 font-mono text-xs font-bold text-violet-600 dark:bg-violet-500/20 dark:text-violet-400">
                        #{hop.hop_index}
                      </span>

                      <span
                        className={`inline-flex items-center rounded-lg border px-2.5 py-0.5 font-mono text-xs font-semibold ${getStatusBadge(
                          hop.status_code,
                        )}`}
                      >
                        {hop.status_code} {hop.status_text}
                      </span>

                      <span className="flex items-center gap-1 font-mono text-xs text-slate-400">
                        <Clock className="h-3 w-3" />
                        <span>{hop.latency_ms} ms</span>
                      </span>
                    </div>

                    <button
                      type="button"
                      onClick={() => copy(hop.url)}
                      className="flex items-center gap-1 rounded-lg border border-slate-200 bg-white px-2 py-1 text-xs text-slate-600 hover:bg-slate-50 dark:border-slate-700 dark:bg-slate-800 dark:text-slate-300"
                    >
                      <Copy className="h-3 w-3" />
                      <span>{copied ? "已复制" : "复制该层 URL"}</span>
                    </button>
                  </div>

                  {/* Hop 完整 URL */}
                  <div className="mt-3 break-all font-mono text-xs font-medium text-slate-800 dark:text-slate-200">
                    {hop.url}
                  </div>

                  {/* 下一跳去向 Location */}
                  {hop.location ? (
                    <div className="mt-3 rounded-xl border border-amber-500/30 bg-amber-500/10 p-3 text-xs dark:border-amber-500/20 dark:bg-amber-950/20">
                      <div className="flex items-center gap-1.5 font-semibold text-amber-700 dark:text-amber-300">
                        <ExternalLink className="h-3.5 w-3.5" />
                        <span>下一跳重定向目标 (Location):</span>
                      </div>
                      <div className="mt-1 break-all font-mono text-amber-900 dark:text-amber-200">
                        {hop.location}
                      </div>
                      {hop.resolved_next_url && hop.resolved_next_url !== hop.location ? (
                        <div className="mt-1 text-[11px] text-amber-700/80 dark:text-amber-400">
                          解析后的绝对地址: {hop.resolved_next_url}
                        </div>
                      ) : null}
                    </div>
                  ) : null}

                  {/* Set-Cookie 拦截 */}
                  {hop.set_cookies.length > 0 && (
                    <div className="mt-3 rounded-xl border border-slate-200/80 bg-slate-50/50 p-3 text-xs dark:border-slate-800 dark:bg-slate-800/40">
                      <div className="flex items-center gap-1.5 font-semibold text-slate-700 dark:text-slate-300">
                        <Cookie className="h-3.5 w-3.5 text-sky-500" />
                        <span>本级写入 Cookie ({hop.set_cookies.length} 条):</span>
                      </div>
                      <ul className="mt-1.5 space-y-1 font-mono text-[11px] text-slate-600 dark:text-slate-400">
                        {hop.set_cookies.map((c, i) => (
                          <li key={i} className="break-all rounded bg-white p-1.5 dark:bg-slate-900">
                            {c}
                          </li>
                        ))}
                      </ul>
                    </div>
                  )}

                  {/* Query 参数表格 */}
                  {queryParams.length > 0 && (
                    <div className="mt-3 rounded-xl border border-slate-200/80 bg-slate-50/50 p-3 text-xs dark:border-slate-800 dark:bg-slate-800/40">
                      <div className="font-semibold text-slate-700 dark:text-slate-300">
                        本级携带 Query 参数 ({queryParams.length} 个):
                      </div>
                      <div className="mt-2 overflow-x-auto">
                        <table className="w-full text-left text-[11px]">
                          <thead>
                            <tr className="border-b border-slate-200/60 text-slate-400 dark:border-slate-700">
                              <th className="pb-1">Key</th>
                              <th className="pb-1">Value</th>
                            </tr>
                          </thead>
                          <tbody className="divide-y divide-slate-200/40 font-mono dark:divide-slate-700/40">
                            {queryParams.map(([k, v], i) => (
                              <tr key={i}>
                                <td className="py-1 pr-2 font-medium text-slate-700 dark:text-slate-300">
                                  {k}
                                </td>
                                <td className="break-all py-1 text-slate-500 dark:text-slate-400">
                                  {v}
                                </td>
                              </tr>
                            ))}
                          </tbody>
                        </table>
                      </div>
                    </div>
                  )}
                </div>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
