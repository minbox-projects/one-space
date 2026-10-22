import { Clock, Trash2 } from "lucide-react";
import type { CapturedWebhookRequest } from "./types";

interface RequestStreamListProps {
  requests: CapturedWebhookRequest[];
  selectedId: string | null;
  onSelect: (req: CapturedWebhookRequest) => void;
  onClear: () => void;
}

export function RequestStreamList({
  requests,
  selectedId,
  onSelect,
  onClear,
}: RequestStreamListProps) {
  const getMethodBadgeClass = (method: string) => {
    switch (method.toUpperCase()) {
      case "POST":
        return "bg-emerald-500/10 text-emerald-600 border-emerald-500/20";
      case "GET":
        return "bg-blue-500/10 text-blue-600 border-blue-500/20";
      case "DELETE":
        return "bg-rose-500/10 text-rose-600 border-rose-500/20";
      case "PUT":
      case "PATCH":
        return "bg-amber-500/10 text-amber-600 border-amber-500/20";
      default:
        return "bg-muted text-muted-foreground border-border";
    }
  };

  const formatTime = (ts: number) => {
    const d = new Date(ts * 1000);
    return d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" });
  };

  const formatSize = (bytes: number) => {
    if (bytes < 1024) return `${bytes} B`;
    return `${(bytes / 1024).toFixed(1)} KB`;
  };

  return (
    <div className="flex flex-col h-full border border-border/70 rounded-xl bg-card overflow-hidden">
      {/* 列表头部 */}
      <div className="flex items-center justify-between px-3 py-2 border-b border-border/50 bg-muted/20 text-xs">
        <span className="font-semibold text-foreground flex items-center gap-1.5">
          <Clock className="w-3.5 h-3.5 text-muted-foreground" />
          请求流水 ({requests.length})
        </span>
        {requests.length > 0 && (
          <button
            type="button"
            onClick={onClear}
            className="text-[11px] text-muted-foreground hover:text-destructive flex items-center gap-1 transition-colors"
            title="清空当前列表"
          >
            <Trash2 className="w-3 h-3" />
            清空
          </button>
        )}
      </div>

      {/* 滚动列表 */}
      <div className="flex-1 overflow-y-auto divide-y divide-border/30">
        {requests.length === 0 ? (
          <div className="p-8 text-center text-xs text-muted-foreground">
            等待接收外部请求...
            <p className="text-[11px] text-muted-foreground/70 mt-1">
              向端点 URL 发送 HTTP 请求即可在此处实时拦截
            </p>
          </div>
        ) : (
          requests.map((req) => {
            const isSelected = req.id === selectedId;
            return (
              <div
                key={req.id}
                onClick={() => onSelect(req)}
                className={`p-2.5 cursor-pointer text-xs transition-colors flex flex-col gap-1 ${
                  isSelected
                    ? "bg-accent/80 border-l-2 border-primary"
                    : "hover:bg-muted/40"
                }`}
              >
                <div className="flex items-center justify-between">
                  <div className="flex items-center gap-1.5">
                    <span
                      className={`px-1.5 py-0.5 rounded text-[10px] font-mono font-bold border ${getMethodBadgeClass(
                        req.method,
                      )}`}
                    >
                      {req.method}
                    </span>
                    <span className="font-mono text-xs text-foreground truncate max-w-[130px]">
                      {req.path}
                    </span>
                  </div>
                  <span className="font-mono text-[10px] text-muted-foreground">
                    {formatTime(req.timestamp)}
                  </span>
                </div>

                <div className="flex items-center justify-between text-[11px] text-muted-foreground">
                  <span className="truncate max-w-[150px]">{req.endpointName}</span>
                  <span className="font-mono text-[10px]">
                    {formatSize(req.body.length)}
                  </span>
                </div>

                {req.detectedSignature && (
                  <div className="mt-0.5">
                    <span className="inline-block px-1.5 py-0.2 rounded text-[10px] bg-purple-500/10 text-purple-600 border border-purple-500/20 font-medium">
                      {req.detectedSignature.provider} 签名
                    </span>
                  </div>
                )}
              </div>
            );
          })
        )}
      </div>
    </div>
  );
}
