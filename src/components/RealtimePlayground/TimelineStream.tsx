import { useRef, useEffect } from "react";
import {
  ArrowDownLeft,
  ArrowUpRight,
  Clock,
  Pause,
  Play,
  RotateCcw,
  Search,
  Zap,
} from "lucide-react";
import type { RealtimeMessage, SseSessionMetrics } from "@/lib/realtimeClient";

interface TimelineStreamProps {
  messages: RealtimeMessage[];
  selectedMessageId: string | null;
  onSelectMessage: (msg: RealtimeMessage) => void;
  onClear: () => void;
  searchFilter: string;
  onSearchChange: (val: string) => void;
  autoScroll: boolean;
  onToggleAutoScroll: () => void;
  sseMetrics: SseSessionMetrics | null;
  isSse: boolean;
}

export function TimelineStream({
  messages,
  selectedMessageId,
  onSelectMessage,
  onClear,
  searchFilter,
  onSearchChange,
  autoScroll,
  onToggleAutoScroll,
  sseMetrics,
  isSse,
}: TimelineStreamProps) {
  const bottomRef = useRef<HTMLDivElement>(null);

  const filtered = messages.filter((m) => {
    if (!searchFilter.trim()) return true;
    const term = searchFilter.toLowerCase();
    return (
      m.content.toLowerCase().includes(term) ||
      (m.event && m.event.toLowerCase().includes(term))
    );
  });

  useEffect(() => {
    if (autoScroll && bottomRef.current) {
      bottomRef.current.scrollIntoView({ behavior: "smooth" });
    }
  }, [messages, autoScroll]);

  return (
    <div className="flex h-full flex-col rounded-2xl border border-slate-200/80 bg-white shadow-xs dark:border-slate-800/80 dark:bg-slate-900">
      {/* 顶部搜索与控制栏 */}
      <div className="flex items-center justify-between border-b border-slate-100 p-3.5 dark:border-slate-800">
        <div className="relative flex-1 max-w-xs">
          <Search className="absolute left-2.5 top-2.5 h-3.5 w-3.5 text-slate-400" />
          <input
            type="text"
            value={searchFilter}
            onChange={(e) => onSearchChange(e.target.value)}
            placeholder="过滤消息内容 / event..."
            className="w-full rounded-lg border border-slate-200/80 bg-slate-50/50 py-1.5 pl-8 pr-3 text-xs text-slate-800 placeholder-slate-400 focus:border-sky-500 focus:bg-white focus:outline-none dark:border-slate-700 dark:bg-slate-800/50 dark:text-slate-200"
          />
        </div>

        <div className="flex items-center gap-1.5">
          <button
            type="button"
            onClick={onToggleAutoScroll}
            title={autoScroll ? "已开启自动滚动" : "已暂停自动滚动（锁屏审查）"}
            className={`flex items-center gap-1 rounded-lg border px-2.5 py-1 text-xs font-medium transition-all ${
              autoScroll
                ? "border-sky-500/30 bg-sky-500/10 text-sky-600 dark:text-sky-400"
                : "border-slate-200 bg-slate-50 text-slate-500 dark:border-slate-700 dark:bg-slate-800"
            }`}
          >
            {autoScroll ? <Play className="h-3 w-3" /> : <Pause className="h-3 w-3" />}
            <span>{autoScroll ? "跟随" : "暂停"}</span>
          </button>

          <button
            type="button"
            onClick={onClear}
            disabled={messages.length === 0}
            title="清空消息流"
            className="rounded-lg p-1.5 text-slate-400 hover:bg-slate-100 hover:text-slate-700 disabled:opacity-30 dark:hover:bg-slate-800 dark:hover:text-slate-200"
          >
            <RotateCcw className="h-3.5 w-3.5" />
          </button>
        </div>
      </div>

      {/* SSE 专属指标横幅 */}
      {isSse && sseMetrics ? (
        <div className="grid grid-cols-3 divide-x divide-slate-100 border-b border-slate-100 bg-sky-50/40 p-2.5 text-center text-xs dark:divide-slate-800 dark:border-slate-800 dark:bg-sky-950/20">
          <div>
            <span className="flex items-center justify-center gap-1 text-[11px] text-slate-400">
              <Zap className="h-3 w-3 text-amber-500" />
              首字延迟 (TTFT)
            </span>
            <span className="font-mono font-semibold text-slate-800 dark:text-slate-100">
              {sseMetrics.ttftMs !== null ? `${Math.round(sseMetrics.ttftMs)} ms` : "等待首字..."}
            </span>
          </div>
          <div>
            <span className="text-[11px] text-slate-400">分块数量 (Chunks)</span>
            <span className="font-mono font-semibold text-sky-600 dark:text-sky-400">
              {sseMetrics.chunkCount}
            </span>
          </div>
          <div>
            <span className="flex items-center justify-center gap-1 text-[11px] text-slate-400">
              <Clock className="h-3 w-3 text-slate-400" />
              持续时长
            </span>
            <span className="font-mono font-semibold text-slate-800 dark:text-slate-100">
              {(sseMetrics.durationMs / 1000).toFixed(1)} s
            </span>
          </div>
        </div>
      ) : null}

      {/* 消息瀑布流列表 */}
      <div className="flex-1 divide-y divide-slate-100 overflow-y-auto p-2 dark:divide-slate-800/60">
        {filtered.length === 0 ? (
          <div className="flex h-48 flex-col items-center justify-center text-center text-xs text-slate-400">
            <Clock className="h-6 w-6 text-slate-300 dark:text-slate-600" />
            <p className="mt-2 font-medium">暂无消息帧</p>
            <p className="text-[11px]">连接成功后，双向传输的消息将在此实时呈现</p>
          </div>
        ) : (
          filtered.map((msg) => {
            const isSelected = msg.id === selectedMessageId;
            const isInbound = msg.direction === "inbound";

            return (
              <button
                key={msg.id}
                type="button"
                onClick={() => onSelectMessage(msg)}
                className={`w-full rounded-xl p-2.5 text-left text-xs transition-all ${
                  isSelected
                    ? "bg-sky-50 ring-1 ring-sky-500/40 dark:bg-sky-950/40 dark:ring-sky-500/40"
                    : "hover:bg-slate-50 dark:hover:bg-slate-800/50"
                }`}
              >
                <div className="flex items-center justify-between">
                  <div className="flex items-center gap-1.5 font-medium">
                    <span
                      className={`inline-flex items-center gap-1 rounded-md px-1.5 py-0.5 text-[10px] font-semibold ${
                        isInbound
                          ? "bg-emerald-500/10 text-emerald-600 dark:bg-emerald-500/20 dark:text-emerald-400"
                          : "bg-sky-500/10 text-sky-600 dark:bg-sky-500/20 dark:text-sky-400"
                      }`}
                    >
                      {isInbound ? (
                        <>
                          <ArrowDownLeft className="h-3 w-3" />
                          <span>接收</span>
                        </>
                      ) : (
                        <>
                          <ArrowUpRight className="h-3 w-3" />
                          <span>发送</span>
                        </>
                      )}
                    </span>

                    {msg.event ? (
                      <span className="rounded bg-violet-100 px-1.5 py-0.2 font-mono text-[10px] font-semibold text-violet-700 dark:bg-violet-900/40 dark:text-violet-300">
                        {msg.event}
                      </span>
                    ) : null}

                    <span className="font-mono text-[11px] text-slate-400">
                      {msg.timeStr}
                    </span>
                  </div>

                  <div className="flex items-center gap-2 font-mono text-[11px] text-slate-400">
                    {msg.deltaMs !== undefined ? (
                      <span>+{msg.deltaMs}ms</span>
                    ) : null}
                    <span>{msg.size} B</span>
                  </div>
                </div>

                <div className="mt-1 line-clamp-2 font-mono text-[11px] text-slate-600 dark:text-slate-300">
                  {msg.content}
                </div>
              </button>
            );
          })
        )}
        <div ref={bottomRef} />
      </div>
    </div>
  );
}
