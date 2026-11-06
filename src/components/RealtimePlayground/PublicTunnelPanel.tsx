import { useState } from "react";
import {
  Activity,
  Check,
  Copy,
  ExternalLink,
  Globe,
  Loader2,
  Play,
  QrCode,
  ShieldCheck,
  Square,
} from "lucide-react";
import { QRCodeSVG } from "qrcode.react";
import { useToast } from "@/components/ToastProvider";
import { useCopyToClipboard } from "@/toolbox/useCopyToClipboard";
import { useTauriEvent } from "@/toolbox/useTauriEvent";
import {
  REALTIME_TUNNEL_EVENT,
  startRealtimeTunnel,
  stopRealtimeTunnel,
  type TunnelRuntimeStatus,
} from "@/lib/realtimeTunnel";

interface PublicTunnelPanelProps {
  onTestInPlayground: (publicUrl: string, protocol: "websocket" | "sse") => void;
}

const COMMON_PORTS = [3000, 8080, 5173, 8000];

export function PublicTunnelPanel({ onTestInPlayground }: PublicTunnelPanelProps) {
  const { pushToast } = useToast();
  const { copy, copied } = useCopyToClipboard();

  const [localPort, setLocalPort] = useState(3000);
  const [protocolType, setProtocolType] = useState<"websocket" | "sse">("websocket");
  const [routePath, setRoutePath] = useState("/ws");
  const [loading, setLoading] = useState(false);
  const [status, setStatus] = useState<TunnelRuntimeStatus | null>(null);

  // 监听穿透状态事件广播
  useTauriEvent(
    REALTIME_TUNNEL_EVENT,
    (payload) => {
      setStatus(payload as TunnelRuntimeStatus);
    },
  );

  const isRunning = status?.status === "running" && !!status.public_url;

  const handleStartTunnel = async () => {
    if (!localPort) {
      pushToast({ title: "请输入有效的本地服务端口", kind: "error" });
      return;
    }

    setLoading(true);
    try {
      const res = await startRealtimeTunnel(localPort);
      setStatus(res);
      pushToast({
        title: "已提交穿透请求，正在拉起 Cloudflare 安全隧道...",
        kind: "info",
      });
    } catch (err) {
      pushToast({
        title: "启动公网映射失败: " + (err instanceof Error ? err.message : String(err)),
        kind: "error",
      });
    } finally {
      setLoading(false);
    }
  };

  const handleStopTunnel = async () => {
    setLoading(true);
    try {
      await stopRealtimeTunnel(localPort);
      setStatus(null);
      pushToast({ title: "已停止公网映射隧道", kind: "info" });
    } catch (err) {
      pushToast({
        title: "停止隧道失败: " + (err instanceof Error ? err.message : String(err)),
        kind: "error",
      });
    } finally {
      setLoading(false);
    }
  };

  // 根据协议与基础 URL 组装最终公开可访问的链接
  const rawBase = status?.public_url?.trim().replace(/\/+$/, "") || "";
  const cleanPath = routePath.startsWith("/") ? routePath : `/${routePath}`;

  const publicWsUrl = rawBase
    ? `${rawBase.replace(/^http:/, "ws:").replace(/^https:/, "wss:")}${cleanPath}`
    : "";

  const publicHttpUrl = rawBase ? `${rawBase}${cleanPath}` : "";

  const activePublicUrl = protocolType === "websocket" ? publicWsUrl : publicHttpUrl;

  return (
    <div className="space-y-5">
      {/* 顶部说明与引导卡片 */}
      <div className="rounded-2xl border border-slate-200/80 bg-white p-5 shadow-xs dark:border-slate-800/80 dark:bg-slate-900">
        <div className="flex items-start justify-between gap-4">
          <div className="flex items-start gap-3">
            <div className="flex h-10 w-10 shrink-0 items-center justify-center rounded-xl bg-sky-500/10 text-sky-600 dark:bg-sky-500/20 dark:text-sky-400">
              <Globe className="h-5 w-5" />
            </div>
            <div>
              <h3 className="text-sm font-semibold text-slate-800 dark:text-slate-100">
                本地服务一键公网穿透映射（基于 Cloudflare Quick Tunnel）
              </h3>
              <p className="mt-1 text-xs text-slate-500 dark:text-slate-400">
                将您电脑本地运行的 WebSocket 或 SSE 服务映射为公网可达的 WSS / HTTPS 地址，免账号配置，支持手机真机扫码或发给协作者进行远程联调测试。
              </p>
            </div>
          </div>

          <span className="inline-flex items-center gap-1 rounded-lg border border-emerald-500/30 bg-emerald-500/10 px-2.5 py-1 text-xs font-semibold text-emerald-600 dark:border-emerald-500/20 dark:text-emerald-400">
            <ShieldCheck className="h-3.5 w-3.5" />
            <span>免账号 / 免费 / 端到端加密</span>
          </span>
        </div>

        {/* 穿透控制表单 */}
        <div className="mt-5 grid grid-cols-1 gap-4 border-t border-slate-100 pt-4 md:grid-cols-3 dark:border-slate-800">
          {/* 本地端口 */}
          <div>
            <label className="mb-1 block text-xs font-semibold text-slate-700 dark:text-slate-300">
              本地服务端口 (Port)
            </label>
            <div className="flex gap-2">
              <input
                type="number"
                min={1}
                max={65535}
                value={localPort}
                onChange={(e) => setLocalPort(Number(e.target.value) || 3000)}
                disabled={isRunning || loading}
                className="w-full rounded-xl border border-slate-200/80 bg-slate-50/50 px-3 py-2 font-mono text-xs text-slate-800 focus:border-sky-500 focus:bg-white focus:outline-none disabled:opacity-50 dark:border-slate-700 dark:bg-slate-800/50 dark:text-slate-100"
              />
            </div>
            <div className="mt-1.5 flex gap-1">
              {COMMON_PORTS.map((p) => (
                <button
                  key={p}
                  type="button"
                  onClick={() => setLocalPort(p)}
                  disabled={isRunning || loading}
                  className="rounded-md border border-slate-200/80 bg-slate-50 px-1.5 py-0.5 font-mono text-[10px] text-slate-600 hover:bg-slate-100 disabled:opacity-40 dark:border-slate-800 dark:bg-slate-800 dark:text-slate-400"
                >
                  :{p}
                </button>
              ))}
            </div>
          </div>

          {/* 目标协议与测试路径 */}
          <div>
            <label className="mb-1 block text-xs font-semibold text-slate-700 dark:text-slate-300">
              协议与路由路径 (Path)
            </label>
            <div className="flex gap-2">
              <select
                value={protocolType}
                onChange={(e) => {
                  const p = e.target.value as "websocket" | "sse";
                  setProtocolType(p);
                  setRoutePath(p === "websocket" ? "/ws" : "/api/events");
                }}
                className="rounded-xl border border-slate-200/80 bg-white px-2.5 py-2 text-xs font-medium dark:border-slate-700 dark:bg-slate-900"
              >
                <option value="websocket">WebSocket (WSS)</option>
                <option value="sse">HTTP / SSE</option>
              </select>

              <input
                type="text"
                value={routePath}
                onChange={(e) => setRoutePath(e.target.value)}
                placeholder="/ws"
                className="flex-1 rounded-xl border border-slate-200/80 bg-slate-50/50 px-3 py-2 font-mono text-xs text-slate-800 focus:border-sky-500 focus:bg-white focus:outline-none dark:border-slate-700 dark:bg-slate-800/50 dark:text-slate-100"
              />
            </div>
          </div>

          {/* 主控按钮 */}
          <div className="flex items-end">
            {isRunning ? (
              <button
                type="button"
                onClick={handleStopTunnel}
                disabled={loading}
                className="flex w-full items-center justify-center gap-1.5 rounded-xl bg-rose-600 py-2.5 text-xs font-semibold text-white shadow-xs hover:bg-rose-500 disabled:opacity-40"
              >
                {loading ? <Loader2 className="h-4 w-4 animate-spin" /> : <Square className="h-4 w-4" />}
                <span>停止公网映射</span>
              </button>
            ) : (
              <button
                type="button"
                onClick={handleStartTunnel}
                disabled={loading}
                className="flex w-full items-center justify-center gap-1.5 rounded-xl bg-sky-600 py-2.5 text-xs font-semibold text-white shadow-xs hover:bg-sky-500 disabled:opacity-40"
              >
                {loading ? <Loader2 className="h-4 w-4 animate-spin" /> : <Play className="h-4 w-4" />}
                <span>开启公网映射</span>
              </button>
            )}
          </div>
        </div>
      </div>

      {/* 公网就绪与分享卡片 */}
      {isRunning ? (
        <div className="grid grid-cols-1 gap-5 md:grid-cols-3">
          {/* 左侧：公网地址与操作 */}
          <div className="space-y-4 md:col-span-2">
            <div className="rounded-2xl border border-emerald-500/30 bg-emerald-500/5 p-5 shadow-xs dark:border-emerald-500/20 dark:bg-emerald-950/10">
              <div className="flex items-center justify-between">
                <span className="flex items-center gap-2 text-xs font-semibold text-emerald-800 dark:text-emerald-300">
                  <span className="h-2.5 w-2.5 rounded-full bg-emerald-500 shadow-xs shadow-emerald-500/50" />
                  <span>公网映射已就绪（全球可达）</span>
                </span>
                <span className="font-mono text-xs text-slate-400">
                  PID: {status.pid || "运行中"}
                </span>
              </div>

              {/* 公网完整地址显示与复制 */}
              <div className="mt-3 flex items-center justify-between gap-2 rounded-xl border border-slate-200/80 bg-white p-3 font-mono text-xs shadow-xs dark:border-slate-800 dark:bg-slate-900">
                <span className="break-all font-semibold text-slate-800 dark:text-slate-100">
                  {activePublicUrl}
                </span>

                <button
                  type="button"
                  onClick={() => copy(activePublicUrl)}
                  className="flex shrink-0 items-center gap-1.5 rounded-lg bg-sky-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-sky-500"
                >
                  {copied ? <Check className="h-3.5 w-3.5" /> : <Copy className="h-3.5 w-3.5" />}
                  <span>{copied ? "已复制" : "复制公网链接"}</span>
                </button>
              </div>

              {/* 快捷测试按钮 */}
              <div className="mt-4 flex flex-wrap items-center gap-3">
                <button
                  type="button"
                  onClick={() => onTestInPlayground(activePublicUrl, protocolType)}
                  className="flex items-center gap-1.5 rounded-xl border border-sky-500/30 bg-white px-3.5 py-2 text-xs font-semibold text-sky-600 shadow-xs hover:bg-sky-50 dark:border-sky-500/20 dark:bg-slate-900 dark:text-sky-400 dark:hover:bg-slate-800"
                >
                  <ExternalLink className="h-3.5 w-3.5" />
                  <span>立即带入客户端演练台自测</span>
                </button>
                <span className="text-[11px] text-slate-400">
                  无需切换外部软件，在同一个窗口内验证您的本地接口
                </span>
              </div>
            </div>
          </div>

          {/* 右侧：手机真机扫码直连 */}
          <div className="flex flex-col items-center justify-center rounded-2xl border border-slate-200/80 bg-white p-5 text-center shadow-xs dark:border-slate-800/80 dark:bg-slate-900">
            <div className="mb-2 flex items-center gap-1.5 text-xs font-semibold text-slate-700 dark:text-slate-300">
              <QrCode className="h-4 w-4 text-sky-500" />
              <span>移动端真机扫码直连测试</span>
            </div>

            <div className="rounded-xl border border-slate-200 p-3 bg-white shadow-xs dark:border-slate-700">
              <QRCodeSVG value={activePublicUrl} size={130} />
            </div>

            <p className="mt-2 text-[11px] text-slate-400">
              手机微信或移动端浏览器扫码即可直接向本地建立连接
            </p>
          </div>
        </div>
      ) : (
        <div className="flex h-48 flex-col items-center justify-center rounded-2xl border border-dashed border-slate-200 bg-white/50 text-center text-xs text-slate-400 dark:border-slate-800 dark:bg-slate-900/50">
          <Activity className="h-8 w-8 text-slate-300 dark:text-slate-600" />
          <p className="mt-2 font-medium">输入本地端口并点击「开启公网映射」</p>
          <p className="text-[11px]">将自动生成全球可直连的 WSS/HTTPS 地址及手机扫码二维码</p>
        </div>
      )}
    </div>
  );
}
