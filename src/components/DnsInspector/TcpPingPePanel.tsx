import { useEffect, useId, useMemo, useState } from "react";
import {
  Activity,
  AlertTriangle,
  CheckCircle2,
  Play,
  RotateCcw,
  Square,
  TrendingUp,
  XCircle,
  Zap,
} from "lucide-react";
import { useToast } from "@/components/ToastProvider";
import { useTauriEvent } from "@/toolbox/useTauriEvent";
import {
  calculateTcpPingStats,
  startTcpPingStream,
  stopTcpPingStream,
  type TcpPingPacket,
} from "@/lib/dnsInspector";

interface PresetTarget {
  name: string;
  host: string;
  port: number;
}

const PRESET_TARGETS: PresetTarget[] = [
  { name: "Cloudflare (1.1.1.1)", host: "1.1.1.1", port: 443 },
  { name: "Google DNS (8.8.8.8)", host: "8.8.8.8", port: 53 },
  { name: "GitHub (SSH)", host: "github.com", port: 22 },
  { name: "Aliyun DNS (223.5.5.5)", host: "223.5.5.5", port: 443 },
];

const COMMON_PORTS = [
  { label: "80 (HTTP)", port: 80 },
  { label: "443 (HTTPS)", port: 443 },
  { label: "22 (SSH)", port: 22 },
  { label: "53 (DNS)", port: 53 },
  { label: "3306 (MySQL)", port: 3306 },
  { label: "6379 (Redis)", port: 6379 },
];

interface TcpPingPePanelProps {
  initialHost?: string;
  initialPort?: number;
}

export function TcpPingPePanel({
  initialHost = "1.1.1.1",
  initialPort = 443,
}: TcpPingPePanelProps) {
  const { pushToast } = useToast();
  const rawSessionId = useId();
  const sessionId = useMemo(
    () => `ping-${rawSessionId.replace(/[^a-zA-Z0-9]/g, "").slice(0, 8)}`,
    [rawSessionId],
  );

  const [host, setHost] = useState(initialHost);
  const [port, setPort] = useState(initialPort);
  const [intervalMs, setIntervalMs] = useState<number>(1000);
  const [probeMode, setProbeMode] = useState<"continuous" | 50 | 100>("continuous");
  const [isRunning, setIsRunning] = useState(false);

  // 探测到的所有包
  const [packets, setPackets] = useState<TcpPingPacket[]>([]);
  // 鼠标悬停的包（用于 Tooltip）
  const [hoveredPacket, setHoveredPacket] = useState<TcpPingPacket | null>(null);

  // 同步外部 initialHost / initialPort
  useEffect(() => {
    if (initialHost) setHost(initialHost);
    if (initialPort) setPort(initialPort);
  }, [initialHost, initialPort]);

  // 监听后端流式发包事件
  const packetEventName = `tcp-ping-packet:${sessionId}`;
  const completeEventName = `tcp-ping-complete:${sessionId}`;

  useTauriEvent(
    packetEventName,
    (payload) => {
      setPackets((prev) => [...prev, payload as TcpPingPacket]);
    },
  );

  useTauriEvent(
    completeEventName,
    () => {
      setIsRunning(false);
      pushToast({
        title: "TCP 握手探测已完成设定轮数",
        kind: "info",
      });
    },
  );

  // 统计指标
  const stats = useMemo(() => calculateTcpPingStats(packets), [packets]);

  // 启动探测
  const handleStart = async () => {
    const cleanHost = host.trim();
    if (!cleanHost) {
      pushToast({ title: "请输入有效的目标 Host 或 IP", kind: "error" });
      return;
    }
    if (!port || port <= 0 || port > 65535) {
      pushToast({ title: "请输入有效的端口号 (1-65535)", kind: "error" });
      return;
    }

    setPackets([]);
    setIsRunning(true);
    try {
      const maxCount = probeMode === "continuous" ? undefined : probeMode;
      await startTcpPingStream(sessionId, cleanHost, port, intervalMs, maxCount, 3000);
      pushToast({
        title: `已启动对 ${cleanHost}:${port} 的 tcp.ping.pe 持续握手探测`,
        kind: "info",
      });
    } catch (err) {
      setIsRunning(false);
      pushToast({
        title: "启动探测失败: " + (err instanceof Error ? err.message : String(err)),
        kind: "error",
      });
    }
  };

  // 停止探测
  const handleStop = async () => {
    try {
      await stopTcpPingStream(sessionId);
    } catch {
      // ignore
    } finally {
      setIsRunning(false);
      pushToast({ title: "已停止握手探测", kind: "info" });
    }
  };

  // 清空
  const handleClear = () => {
    if (isRunning) {
      void handleStop();
    }
    setPackets([]);
    setHoveredPacket(null);
  };

  // 组件卸载时确保停止
  useEffect(() => {
    return () => {
      void stopTcpPingStream(sessionId);
    };
  }, [sessionId]);

  // 获取方块颜色
  const getSquareColor = (p: TcpPingPacket) => {
    if (!p.success || typeof p.latency_ms !== "number") {
      return "bg-rose-600 hover:bg-rose-700 text-white";
    }
    const ms = p.latency_ms;
    if (ms < 50) return "bg-emerald-500 hover:bg-emerald-400";
    if (ms < 100) return "bg-teal-500 hover:bg-teal-400";
    if (ms < 200) return "bg-amber-500 hover:bg-amber-400";
    return "bg-rose-500 hover:bg-rose-400";
  };

  // 最近 40 个有效包的折线图坐标
  const chartData = useMemo(() => {
    const recent = packets.slice(-40);
    if (recent.length < 2) return null;
    const maxVal = Math.max(...recent.map((p) => p.latency_ms || 0), 10);
    const minVal = 0;
    const width = 480;
    const height = 64;
    const padding = 6;

    const points = recent.map((p, idx) => {
      const x = padding + (idx / (recent.length - 1)) * (width - padding * 2);
      const val = p.success && p.latency_ms !== null && p.latency_ms !== undefined ? p.latency_ms : maxVal;
      const y = height - padding - ((val - minVal) / (maxVal - minVal)) * (height - padding * 2);
      return { x, y, val, success: p.success };
    });

    const pathD = points.reduce((acc, pt, i) => {
      return i === 0 ? `M ${pt.x} ${pt.y}` : `${acc} L ${pt.x} ${pt.y}`;
    }, "");

    return { points, pathD, width, height, maxVal };
  }, [packets]);

  return (
    <div className="space-y-5">
      {/* 顶部控制面板 */}
      <div className="rounded-2xl border border-slate-200/80 bg-white p-5 shadow-xs dark:border-slate-800 dark:bg-slate-900">
        <div className="flex flex-wrap items-center justify-between gap-4">
          <div className="flex flex-wrap items-center gap-2">
            <span className="text-xs font-semibold text-slate-700 dark:text-slate-300">
              快捷目标:
            </span>
            {PRESET_TARGETS.map((t) => (
              <button
                key={t.name}
                type="button"
                onClick={() => {
                  setHost(t.host);
                  setPort(t.port);
                }}
                disabled={isRunning}
                className="rounded-lg border border-slate-200 bg-slate-50 px-2.5 py-1 text-xs text-slate-600 transition-colors hover:border-indigo-300 hover:bg-indigo-50 hover:text-indigo-600 disabled:opacity-40 dark:border-slate-800 dark:bg-slate-800/80 dark:text-slate-300 dark:hover:border-indigo-700 dark:hover:text-indigo-400"
              >
                {t.name}
              </button>
            ))}
          </div>

          {/* 模式选择 */}
          <div className="flex items-center gap-2">
            <span className="text-xs text-slate-500 dark:text-slate-400">间隔:</span>
            <div className="flex rounded-xl bg-slate-100 p-0.5 dark:bg-slate-800">
              {[
                { label: "500ms", val: 500 },
                { label: "1s", val: 1000 },
                { label: "2s", val: 2000 },
              ].map((item) => (
                <button
                  key={item.val}
                  type="button"
                  disabled={isRunning}
                  onClick={() => setIntervalMs(item.val)}
                  className={`rounded-lg px-2 py-0.5 text-xs font-medium transition-all ${
                    intervalMs === item.val
                      ? "bg-white text-indigo-600 shadow-xs dark:bg-slate-700 dark:text-indigo-400"
                      : "text-slate-500 hover:text-slate-800 dark:text-slate-400"
                  }`}
                >
                  {item.label}
                </button>
              ))}
            </div>

            <span className="ml-2 text-xs text-slate-500 dark:text-slate-400">轮数:</span>
            <div className="flex rounded-xl bg-slate-100 p-0.5 dark:bg-slate-800">
              {[
                { label: "持续探测", val: "continuous" as const },
                { label: "50次", val: 50 as const },
                { label: "100次", val: 100 as const },
              ].map((item) => (
                <button
                  key={String(item.val)}
                  type="button"
                  disabled={isRunning}
                  onClick={() => setProbeMode(item.val)}
                  className={`rounded-lg px-2 py-0.5 text-xs font-medium transition-all ${
                    probeMode === item.val
                      ? "bg-white text-indigo-600 shadow-xs dark:bg-slate-700 dark:text-indigo-400"
                      : "text-slate-500 hover:text-slate-800 dark:text-slate-400"
                  }`}
                >
                  {item.label}
                </button>
              ))}
            </div>
          </div>
        </div>

        {/* 目标输入与发包主控 */}
        <div className="mt-4 flex flex-wrap items-center gap-3">
          {/* Host 输入 */}
          <div className="relative min-w-64 flex-1">
            <span className="absolute left-3 top-2.5 font-mono text-xs font-bold text-slate-400">
              Host:
            </span>
            <input
              type="text"
              value={host}
              disabled={isRunning}
              onChange={(e) => setHost(e.target.value)}
              placeholder="IP 或 域名 (如 1.1.1.1 或 example.com)"
              className="w-full rounded-xl border border-slate-200/80 bg-slate-50/50 py-2 pl-14 pr-3 font-mono text-xs text-slate-800 shadow-xs focus:border-indigo-500 focus:bg-white focus:outline-none disabled:opacity-60 dark:border-slate-800 dark:bg-slate-800/50 dark:text-slate-100 dark:focus:border-indigo-400"
            />
          </div>

          {/* Port 输入 */}
          <div className="relative w-28">
            <span className="absolute left-3 top-2.5 font-mono text-xs font-bold text-slate-400">
              Port:
            </span>
            <input
              type="number"
              value={port}
              disabled={isRunning}
              onChange={(e) => setPort(Number(e.target.value))}
              className="w-full rounded-xl border border-slate-200/80 bg-slate-50/50 py-2 pl-14 pr-2 font-mono text-xs text-slate-800 shadow-xs focus:border-indigo-500 focus:bg-white focus:outline-none disabled:opacity-60 dark:border-slate-800 dark:bg-slate-800/50 dark:text-slate-100 dark:focus:border-indigo-400"
            />
          </div>

          {/* 常用端口快捷下拉 */}
          <div className="flex items-center gap-1">
            {COMMON_PORTS.slice(0, 4).map((p) => (
              <button
                key={p.port}
                type="button"
                disabled={isRunning}
                onClick={() => setPort(p.port)}
                className={`rounded-lg px-2 py-1 text-[11px] font-mono transition-all ${
                  port === p.port
                    ? "bg-indigo-50 text-indigo-700 border border-indigo-200 dark:bg-indigo-950/40 dark:text-indigo-300 dark:border-indigo-800"
                    : "text-slate-500 hover:bg-slate-100 dark:hover:bg-slate-800"
                }`}
              >
                :{p.port}
              </button>
            ))}
          </div>

          {/* 主控按钮 */}
          <div className="flex items-center gap-2">
            {isRunning ? (
              <button
                type="button"
                onClick={handleStop}
                className="flex items-center gap-1.5 rounded-xl bg-rose-600 px-4 py-2 text-xs font-semibold text-white shadow-xs transition-all hover:bg-rose-500"
              >
                <Square className="h-3.5 w-3.5 fill-current" />
                <span>停止探测</span>
              </button>
            ) : (
              <button
                type="button"
                onClick={handleStart}
                className="flex items-center gap-1.5 rounded-xl bg-indigo-600 px-4 py-2 text-xs font-semibold text-white shadow-xs transition-all hover:bg-indigo-500"
              >
                <Play className="h-3.5 w-3.5 fill-current" />
                <span>开始探测</span>
              </button>
            )}

            <button
              type="button"
              onClick={handleClear}
              disabled={packets.length === 0}
              className="flex items-center gap-1 rounded-xl border border-slate-200/80 bg-white px-3 py-2 text-xs font-medium text-slate-600 shadow-xs hover:bg-slate-50 disabled:opacity-40 dark:border-slate-800 dark:bg-slate-900 dark:text-slate-400 dark:hover:bg-slate-800"
            >
              <RotateCcw className="h-3.5 w-3.5" />
              <span>清空</span>
            </button>
          </div>
        </div>
      </div>

      {/* 四大指标看板 (Loss% / Min / Avg / Max / Jitter) */}
      <div className="grid grid-cols-2 gap-4 sm:grid-cols-3 lg:grid-cols-6">
        {/* Loss% 丢包率 */}
        <div className="rounded-2xl border border-slate-200/80 bg-white p-4 shadow-xs dark:border-slate-800 dark:bg-slate-900">
          <div className="flex items-center justify-between">
            <span className="text-xs font-medium text-slate-500 dark:text-slate-400">
              Loss (丢包率)
            </span>
            <div
              className={`flex h-6 w-6 items-center justify-center rounded-lg ${
                stats.lossRate > 0
                  ? "bg-rose-500/10 text-rose-600 dark:bg-rose-500/20 dark:text-rose-400"
                  : "bg-emerald-500/10 text-emerald-600 dark:bg-emerald-500/20 dark:text-emerald-400"
              }`}
            >
              {stats.lossRate > 0 ? (
                <XCircle className="h-3.5 w-3.5" />
              ) : (
                <CheckCircle2 className="h-3.5 w-3.5" />
              )}
            </div>
          </div>
          <div className="mt-2 flex items-baseline gap-1">
            <span
              className={`font-mono text-2xl font-bold tracking-tight ${
                stats.lossRate > 0 ? "text-rose-600 dark:text-rose-400" : "text-emerald-600 dark:text-emerald-400"
              }`}
            >
              {stats.lossRate}%
            </span>
          </div>
          <p className="mt-1 text-[11px] text-slate-400">
            {stats.lost} 丢包 / {stats.sent} 发送
          </p>
        </div>

        {/* Min 最小握手 */}
        <div className="rounded-2xl border border-slate-200/80 bg-white p-4 shadow-xs dark:border-slate-800 dark:bg-slate-900">
          <div className="flex items-center justify-between">
            <span className="text-xs font-medium text-slate-500 dark:text-slate-400">
              Min (最小时延)
            </span>
            <div className="flex h-6 w-6 items-center justify-center rounded-lg bg-teal-500/10 text-teal-600 dark:bg-teal-500/20 dark:text-teal-400">
              <Zap className="h-3.5 w-3.5" />
            </div>
          </div>
          <div className="mt-2 flex items-baseline gap-1">
            <span className="font-mono text-2xl font-bold tracking-tight text-slate-900 dark:text-white">
              {stats.min}
            </span>
            <span className="text-xs text-slate-400">ms</span>
          </div>
          <p className="mt-1 text-[11px] text-slate-400">最佳探测表现</p>
        </div>

        {/* Avg 平均握手 */}
        <div className="rounded-2xl border border-slate-200/80 bg-white p-4 shadow-xs dark:border-slate-800 dark:bg-slate-900">
          <div className="flex items-center justify-between">
            <span className="text-xs font-medium text-slate-500 dark:text-slate-400">
              Avg (平均时延)
            </span>
            <div className="flex h-6 w-6 items-center justify-center rounded-lg bg-indigo-500/10 text-indigo-600 dark:bg-indigo-500/20 dark:text-indigo-400">
              <Activity className="h-3.5 w-3.5" />
            </div>
          </div>
          <div className="mt-2 flex items-baseline gap-1">
            <span className="font-mono text-2xl font-bold tracking-tight text-indigo-600 dark:text-indigo-400">
              {stats.avg}
            </span>
            <span className="text-xs text-slate-400">ms</span>
          </div>
          <p className="mt-1 text-[11px] text-slate-400">统计加权平均</p>
        </div>

        {/* Max 最大握手 */}
        <div className="rounded-2xl border border-slate-200/80 bg-white p-4 shadow-xs dark:border-slate-800 dark:bg-slate-900">
          <div className="flex items-center justify-between">
            <span className="text-xs font-medium text-slate-500 dark:text-slate-400">
              Max (最劣时延)
            </span>
            <div className="flex h-6 w-6 items-center justify-center rounded-lg bg-amber-500/10 text-amber-600 dark:bg-amber-500/20 dark:text-amber-400">
              <AlertTriangle className="h-3.5 w-3.5" />
            </div>
          </div>
          <div className="mt-2 flex items-baseline gap-1">
            <span className="font-mono text-2xl font-bold tracking-tight text-slate-900 dark:text-white">
              {stats.max}
            </span>
            <span className="text-xs text-slate-400">ms</span>
          </div>
          <p className="mt-1 text-[11px] text-slate-400">峰值网络抖动</p>
        </div>

        {/* Jitter 抖动 */}
        <div className="rounded-2xl border border-slate-200/80 bg-white p-4 shadow-xs dark:border-slate-800 dark:bg-slate-900">
          <div className="flex items-center justify-between">
            <span className="text-xs font-medium text-slate-500 dark:text-slate-400">
              Jitter (抖动)
            </span>
            <div className="flex h-6 w-6 items-center justify-center rounded-lg bg-sky-500/10 text-sky-600 dark:bg-sky-500/20 dark:text-sky-400">
              <TrendingUp className="h-3.5 w-3.5" />
            </div>
          </div>
          <div className="mt-2 flex items-baseline gap-1">
            <span className="font-mono text-2xl font-bold tracking-tight text-slate-900 dark:text-white">
              ±{stats.jitter}
            </span>
            <span className="text-xs text-slate-400">ms</span>
          </div>
          <p className="mt-1 text-[11px] text-slate-400">相邻包波动差</p>
        </div>

        {/* Last / Status */}
        <div className="rounded-2xl border border-slate-200/80 bg-white p-4 shadow-xs dark:border-slate-800 dark:bg-slate-900">
          <div className="flex items-center justify-between">
            <span className="text-xs font-medium text-slate-500 dark:text-slate-400">
              Last (最新)
            </span>
            <span
              className={`h-2.5 w-2.5 rounded-full ${
                isRunning
                  ? "animate-pulse bg-emerald-500 shadow-xs shadow-emerald-500/50"
                  : "bg-slate-300 dark:bg-slate-600"
              }`}
            />
          </div>
          <div className="mt-2 flex items-baseline gap-1">
            <span className="font-mono text-2xl font-bold tracking-tight text-slate-900 dark:text-white">
              {stats.last}
            </span>
            <span className="text-xs text-slate-400">ms</span>
          </div>
          <p className="mt-1 text-[11px] text-slate-400">
            {isRunning ? "探测活跃中" : "就绪 / 已停止"}
          </p>
        </div>
      </div>

      {/* 核心检测视窗：对齐 tcp.ping.pe 的高保真实时心跳方块流 (Heartbeat Squares Stream) */}
      <div className="rounded-2xl border border-slate-200/80 bg-white p-6 shadow-xs dark:border-slate-800 dark:bg-slate-900">
        <div className="flex flex-wrap items-center justify-between gap-4 border-b border-slate-100 pb-4 dark:border-slate-800">
          <div className="flex items-center gap-2">
            <Activity className="h-4 w-4 text-indigo-600 dark:text-indigo-400" />
            <h2 className="text-sm font-semibold text-slate-900 dark:text-white">
              实时握手心跳方块流 (Heartbeat Squares)
            </h2>
            <span className="rounded-full bg-slate-100 px-2 py-0.5 font-mono text-[11px] text-slate-600 dark:bg-slate-800 dark:text-slate-300">
              {packets.length} 包
            </span>
          </div>

          {/* 方块图例颜色说明 */}
          <div className="flex items-center gap-3 text-[11px] text-slate-500 dark:text-slate-400">
            <div className="flex items-center gap-1.5">
              <span className="h-3 w-3 rounded-xs bg-emerald-500" />
              <span>&lt;50ms</span>
            </div>
            <div className="flex items-center gap-1.5">
              <span className="h-3 w-3 rounded-xs bg-teal-500" />
              <span>50-100ms</span>
            </div>
            <div className="flex items-center gap-1.5">
              <span className="h-3 w-3 rounded-xs bg-amber-500" />
              <span>100-200ms</span>
            </div>
            <div className="flex items-center gap-1.5">
              <span className="h-3 w-3 rounded-xs bg-rose-500" />
              <span>&gt;200ms</span>
            </div>
            <div className="flex items-center gap-1.5">
              <span className="h-3 w-3 rounded-xs bg-rose-700" />
              <span>超时/丢包</span>
            </div>
          </div>
        </div>

        {/* 方块矩阵视口 */}
        <div className="relative mt-5">
          {packets.length === 0 ? (
            <div className="flex flex-col items-center justify-center rounded-xl border border-dashed border-slate-200 py-16 text-center dark:border-slate-800">
              <div className="flex h-12 w-12 items-center justify-center rounded-full bg-indigo-50 text-indigo-600 dark:bg-indigo-950/40 dark:text-indigo-400">
                <Play className="h-6 w-6 fill-current pl-0.5" />
              </div>
              <p className="mt-3 text-sm font-medium text-slate-700 dark:text-slate-300">
                点击上方“开始探测”发起对 {host}:{port} 的持续 TCP 握手
              </p>
              <p className="mt-1 text-xs text-slate-400">
                将以 500ms~1000ms 间隔持续发包，生成类似 tcp.ping.pe 的高密度矩阵与指标
              </p>
            </div>
          ) : (
            <div className="flex flex-wrap gap-1.5">
              {packets.map((p, idx) => {
                const isLatest = idx === packets.length - 1 && isRunning;
                return (
                  <div
                    key={`${p.session_id}-${p.seq}`}
                    onMouseEnter={() => setHoveredPacket(p)}
                    onMouseLeave={() => setHoveredPacket(null)}
                    className={`relative flex h-5 w-5 cursor-pointer items-center justify-center rounded-xs font-mono text-[9px] font-bold transition-transform hover:scale-125 hover:z-20 ${getSquareColor(
                      p,
                    )} ${isLatest ? "ring-2 ring-indigo-400 ring-offset-1 animate-pulse" : ""}`}
                  >
                    {!p.success && <span className="text-[10px] leading-none">×</span>}
                  </div>
                );
              })}
            </div>
          )}

          {/* Hover 悬停 Tooltip 气泡 */}
          {hoveredPacket && (
            <div className="pointer-events-none mt-4 flex items-center justify-between rounded-xl border border-slate-200 bg-slate-900/90 px-4 py-2.5 text-xs text-white shadow-lg backdrop-blur-md dark:border-slate-700">
              <div className="flex items-center gap-3">
                <span className="font-mono font-bold text-indigo-300">
                  #{hoveredPacket.seq}
                </span>
                <span>
                  目标: <code className="font-mono">{hoveredPacket.host}:{hoveredPacket.port}</code>
                </span>
                <span>
                  状态:{" "}
                  <strong
                    className={
                      hoveredPacket.success ? "text-emerald-400" : "text-rose-400"
                    }
                  >
                    {hoveredPacket.success ? "握手成功" : "握手失败"}
                  </strong>
                </span>
                {hoveredPacket.latency_ms !== null && hoveredPacket.latency_ms !== undefined && (
                  <span>
                    时延:{" "}
                    <strong className="font-mono text-amber-300">
                      {hoveredPacket.latency_ms} ms
                    </strong>
                  </span>
                )}
                {hoveredPacket.error && (
                  <span className="text-rose-300">错误: {hoveredPacket.error}</span>
                )}
              </div>
              <span className="text-[11px] text-slate-400">
                {new Date(hoveredPacket.timestamp).toLocaleTimeString()}
              </span>
            </div>
          )}
        </div>

        {/* 延迟波动 Mini-Trend 折线走势 */}
        {chartData && (
          <div className="mt-6 border-t border-slate-100 pt-5 dark:border-slate-800">
            <div className="flex items-center justify-between text-xs text-slate-500 dark:text-slate-400">
              <div className="flex items-center gap-1.5">
                <TrendingUp className="h-3.5 w-3.5 text-indigo-500" />
                <span className="font-medium text-slate-700 dark:text-slate-300">
                  最近 40 个探针时延走势 (RTT Trend)
                </span>
              </div>
              <span className="font-mono text-[11px]">峰值: {chartData.maxVal} ms</span>
            </div>

            <div className="mt-2 h-16 w-full overflow-hidden rounded-xl bg-slate-50 p-1 dark:bg-slate-800/40">
              <svg
                viewBox={`0 0 ${chartData.width} ${chartData.height}`}
                className="h-full w-full overflow-visible"
              >
                <path
                  d={chartData.pathD}
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="2"
                  className="text-indigo-500 dark:text-indigo-400 transition-all duration-200"
                />
                {chartData.points.map((pt, i) => (
                  <circle
                    key={i}
                    cx={pt.x}
                    cy={pt.y}
                    r={pt.success ? "2" : "3"}
                    className={
                      pt.success
                        ? "fill-indigo-600 dark:fill-indigo-400"
                        : "fill-rose-600 dark:fill-rose-400"
                    }
                  />
                ))}
              </svg>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
