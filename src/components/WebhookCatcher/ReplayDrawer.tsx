import { useState } from "react";
import { Loader2, Play, Plus, Trash2, X } from "lucide-react";
import type { CapturedWebhookRequest, ReplayRequest, ReplayResult } from "./types";

interface ReplayDrawerProps {
  request: CapturedWebhookRequest | null;
  isOpen: boolean;
  onClose: () => void;
  onExecuteReplay: (req: ReplayRequest) => Promise<ReplayResult>;
}

export function ReplayDrawer({
  request,
  isOpen,
  onClose,
  onExecuteReplay,
}: ReplayDrawerProps) {
  const [targetUrl, setTargetUrl] = useState("http://localhost:3000/api/webhook");
  const [overrideBody, setOverrideBody] = useState(request?.body || "");
  const [headersList, setHeadersList] = useState<Array<{ key: string; value: string }>>(() => {
    if (!request) return [];
    return Object.entries(request.headers).map(([key, value]) => ({ key, value }));
  });

  const [executing, setExecuting] = useState(false);
  const [result, setResult] = useState<ReplayResult | null>(null);

  if (!isOpen || !request) return null;

  const handleRun = async () => {
    if (!targetUrl.trim()) return;
    setExecuting(true);
    setResult(null);

    const headersMap: Record<string, string> = {};
    for (const item of headersList) {
      if (item.key.trim()) {
        headersMap[item.key.trim()] = item.value;
      }
    }

    try {
      const res = await onExecuteReplay({
        requestId: request.id,
        targetUrl: targetUrl.trim(),
        overrideHeaders: headersMap,
        overrideBody,
      });
      setResult(res);
    } finally {
      setExecuting(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm p-4 animate-in fade-in-0 duration-150">
      <div className="relative w-full max-w-xl rounded-xl border border-border bg-card p-6 shadow-xl space-y-4 max-h-[90vh] overflow-y-auto text-xs">
        <div className="flex items-center justify-between border-b border-border/40 pb-3">
          <h2 className="text-base font-bold text-foreground">
            改包并重放 (Mutate & Replay)
          </h2>
          <button
            type="button"
            onClick={onClose}
            className="rounded-md p-1 text-muted-foreground hover:bg-muted hover:text-foreground"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        <div className="space-y-3">
          <div>
            <label className="block text-muted-foreground mb-1 font-medium">
              重放目标 URL
            </label>
            <input
              type="text"
              required
              value={targetUrl}
              onChange={(e) => setTargetUrl(e.target.value)}
              placeholder="http://localhost:3000/api/webhook"
              className="w-full font-mono rounded-md border border-input bg-background px-3 py-1.5 text-xs text-foreground focus:outline-none focus:ring-1 focus:ring-ring"
            />
          </div>

          <div>
            <div className="flex items-center justify-between mb-1">
              <label className="text-muted-foreground font-medium">
                Headers 微调
              </label>
              <button
                type="button"
                onClick={() => setHeadersList([...headersList, { key: "", value: "" }])}
                className="text-[11px] text-primary hover:underline flex items-center gap-1"
              >
                <Plus className="w-3 h-3" />
                添加 Header
              </button>
            </div>
            <div className="max-h-36 overflow-y-auto space-y-1.5 border border-border/50 rounded-md p-2 bg-muted/10">
              {headersList.map((h, idx) => (
                <div key={idx} className="flex items-center gap-2">
                  <input
                    type="text"
                    value={h.key}
                    onChange={(e) => {
                      const next = [...headersList];
                      next[idx].key = e.target.value;
                      setHeadersList(next);
                    }}
                    placeholder="Header Key"
                    className="flex-1 font-mono rounded border border-input bg-background px-2 py-1 text-[11px]"
                  />
                  <input
                    type="text"
                    value={h.value}
                    onChange={(e) => {
                      const next = [...headersList];
                      next[idx].value = e.target.value;
                      setHeadersList(next);
                    }}
                    placeholder="Header Value"
                    className="flex-1 font-mono rounded border border-input bg-background px-2 py-1 text-[11px]"
                  />
                  <button
                    type="button"
                    onClick={() => {
                      const next = headersList.filter((_, i) => i !== idx);
                      setHeadersList(next);
                    }}
                    className="text-muted-foreground hover:text-destructive p-1"
                  >
                    <Trash2 className="w-3.5 h-3.5" />
                  </button>
                </div>
              ))}
            </div>
          </div>

          <div>
            <label className="block text-muted-foreground mb-1 font-medium">
              Payload / Body 微调
            </label>
            <textarea
              rows={8}
              value={overrideBody}
              onChange={(e) => setOverrideBody(e.target.value)}
              className="w-full font-mono rounded-md border border-input bg-background p-2.5 text-xs text-foreground focus:outline-none focus:ring-1 focus:ring-ring"
            />
          </div>

          {result && (
            <div className="rounded-lg border border-border/60 bg-muted/20 p-3 space-y-1 font-mono text-xs">
              <div className="flex items-center justify-between">
                <span className="font-bold text-emerald-600">
                  HTTP {result.statusCode}
                </span>
                <span className="text-muted-foreground">{result.durationMs}ms</span>
              </div>
              <pre className="whitespace-pre-wrap break-all text-[11px] max-h-32 overflow-y-auto bg-background p-2 rounded border border-border/40">
                {result.responseBody}
              </pre>
            </div>
          )}

          <div className="flex items-center justify-end gap-2 pt-2 border-t border-border/40">
            <button
              type="button"
              onClick={onClose}
              className="rounded-md border border-border px-3 py-1.5 text-muted-foreground hover:bg-accent transition-colors"
            >
              关闭
            </button>
            <button
              type="button"
              disabled={executing}
              onClick={handleRun}
              className="inline-flex items-center gap-1.5 rounded-md bg-primary px-4 py-1.5 font-medium text-primary-foreground hover:bg-primary/90 transition-colors disabled:opacity-50"
            >
              {executing ? (
                <Loader2 className="w-3.5 h-3.5 animate-spin" />
              ) : (
                <Play className="w-3.5 h-3.5" />
              )}
              发送重放
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
