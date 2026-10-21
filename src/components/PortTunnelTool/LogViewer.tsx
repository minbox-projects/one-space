import { useState, useRef, useEffect, useCallback } from "react";
import { useTranslation } from "react-i18next";
import { Terminal, Trash2, ChevronDown, ChevronRight, ArrowDown } from "lucide-react";
import { Button } from "@/components/ui/button";

interface LogViewerProps {
  logs: string[];
  onClear: () => void;
}

const BOTTOM_THRESHOLD = 30; // 滚动到底部允许的容差像素

export function LogViewer({ logs, onClear }: LogViewerProps) {
  const { t } = useTranslation();
  const [expanded, setExpanded] = useState(true);
  const [isAtBottom, setIsAtBottom] = useState(true);
  const logContainerRef = useRef<HTMLDivElement>(null);
  const isAtBottomRef = useRef(true);

  // 检查当前滚动位置是否在底部
  const handleScroll = useCallback(() => {
    const el = logContainerRef.current;
    if (!el) return;
    const distanceToBottom = el.scrollHeight - el.scrollTop - el.clientHeight;
    const atBottom = distanceToBottom <= BOTTOM_THRESHOLD;
    isAtBottomRef.current = atBottom;
    setIsAtBottom(atBottom);
  }, []);

  // 滚动到底部
  const scrollToBottom = useCallback((smooth = false) => {
    const el = logContainerRef.current;
    if (!el) return;
    if (typeof el.scrollTo === "function") {
      el.scrollTo({
        top: el.scrollHeight,
        behavior: smooth ? "smooth" : "auto",
      });
    } else {
      el.scrollTop = el.scrollHeight;
    }
    isAtBottomRef.current = true;
    setIsAtBottom(true);
  }, []);

  // 仅当用户原本就在底部时，新日志追加才自动滚动到底部；如果用户向上翻阅历史日志，则保持滚动位置不跳动
  useEffect(() => {
    if (expanded && isAtBottomRef.current) {
      scrollToBottom(false);
    }
  }, [logs, expanded, scrollToBottom]);

  const handleClear = () => {
    onClear();
    isAtBottomRef.current = true;
    setIsAtBottom(true);
  };

  return (
    <div className="rounded-xl border border-border/80 bg-card overflow-hidden shadow-sm">
      <div className="flex items-center justify-between border-b border-border/60 bg-muted/40 px-3.5 py-2.5">
        <button
          type="button"
          onClick={() => setExpanded(!expanded)}
          className="flex items-center gap-2 text-xs font-semibold text-foreground/80 hover:text-foreground transition-colors"
        >
          {expanded ? (
            <ChevronDown className="h-4 w-4 text-muted-foreground" />
          ) : (
            <ChevronRight className="h-4 w-4 text-muted-foreground" />
          )}
          <Terminal className="h-4 w-4 text-emerald-500" />
          <span>{t("portTunnelLogs", "Real-time Logs")}</span>
          <span className="rounded-full bg-muted px-2 py-0.5 text-[10px] text-muted-foreground font-mono">
            {logs.length}
          </span>
        </button>

        {expanded && logs.length > 0 && (
          <Button
            variant="ghost"
            size="sm"
            onClick={handleClear}
            className="h-7 px-2 text-xs text-muted-foreground hover:text-foreground gap-1"
          >
            <Trash2 className="h-3.5 w-3.5" />
            <span>{t("portTunnelClearLogs", "Clear")}</span>
          </Button>
        )}
      </div>

      {expanded && (
        <div className="relative">
          <div
            ref={logContainerRef}
            onScroll={handleScroll}
            className="h-48 overflow-y-auto bg-slate-950/95 p-3 font-mono text-[11px] leading-relaxed text-slate-300 dark:bg-black/95 select-text"
          >
            {logs.length === 0 ? (
              <div className="flex h-full items-center justify-center text-slate-600">
                {t("portTunnelNoLogs", "No logs recorded yet.")}
              </div>
            ) : (
              logs.map((line, idx) => (
                <div key={idx} className="whitespace-pre-wrap break-all hover:bg-white/5 py-0.5 px-1 rounded">
                  <span className="text-slate-400">{line}</span>
                </div>
              ))
            )}
          </div>

          {/* 向上滚动查看历史日志时出现的浮动回到底部按钮 */}
          {!isAtBottom && logs.length > 0 && (
            <button
              type="button"
              onClick={() => scrollToBottom(true)}
              data-testid="port-tunnel-scroll-latest"
              className="absolute bottom-2.5 right-3 flex items-center gap-1 rounded-full bg-slate-800/90 px-2.5 py-1 text-[11px] font-medium text-slate-200 shadow-md backdrop-blur-sm border border-slate-700/80 hover:bg-slate-700 hover:text-white transition-all cursor-pointer"
              title={t("portTunnelToLatest", "Scroll to latest")}
            >
              <ArrowDown className="h-3 w-3 text-emerald-400" />
              <span>{t("portTunnelToLatest", "To Latest")}</span>
            </button>
          )}
        </div>
      )}
    </div>
  );
}
