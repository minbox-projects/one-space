import { useMemo, useState } from "react";
import { Check, Copy, Eye, EyeOff, Play, ShieldCheck } from "lucide-react";
import { useCopyToClipboard } from "@/toolbox/useCopyToClipboard";
import { useToast } from "@/components/ToastProvider";
import type { CapturedWebhookRequest, ReplayResult } from "./types";

function maskObject(obj: unknown): unknown {
  if (typeof obj !== "object" || obj === null) return obj;
  if (Array.isArray(obj)) return obj.map(maskObject);

  const masked: Record<string, unknown> = {};
  for (const [k, v] of Object.entries(obj as Record<string, unknown>)) {
    const lower = k.toLowerCase();
    if (
      (lower.includes("password") ||
        lower.includes("secret") ||
        lower.includes("token") ||
        lower.includes("key")) &&
      typeof v === "string"
    ) {
      masked[k] = v.length > 8 ? `${v.slice(0, 3)}******${v.slice(-3)}` : "******";
    } else {
      masked[k] = maskObject(v);
    }
  }
  return masked;
}

interface RequestInspectorProps {
  request: CapturedWebhookRequest | null;
  onInstantReplay: (targetUrl: string) => Promise<ReplayResult>;
  onOpenReplayDrawer: () => void;
  isReplaying?: boolean;
  replayResult?: ReplayResult | null;
}

export function RequestInspector({
  request,
  onInstantReplay,
  onOpenReplayDrawer,
  isReplaying = false,
  replayResult,
}: RequestInspectorProps) {
  const { pushToast } = useToast();
  const [activeTab, setActiveTab] = useState<"body" | "headers" | "query">("body");
  const [maskSensitive, setMaskSensitive] = useState(false);
  const [replayUrl, setReplayUrl] = useState("http://localhost:3000/api/webhook");

  const { copied: copiedBody, copy: copyBody } = useCopyToClipboard({
    onSuccess: () =>
      pushToast({
        title: "已复制请求体 Payload",
        kind: "success",
      }),
  });

  const { copied: copiedHeaders, copy: copyHeaders } = useCopyToClipboard({
    onSuccess: () =>
      pushToast({
        title: "已复制 Headers",
        kind: "success",
      }),
  });

  const displayedBody = useMemo(() => {
    if (!request || !request.body) return "";
    let content = request.body;
    try {
      if (request.bodyFormat === "json") {
        const parsed = JSON.parse(request.body);
        if (maskSensitive) {
          content = JSON.stringify(maskObject(parsed), null, 2);
        } else {
          content = JSON.stringify(parsed, null, 2);
        }
      }
    } catch {
      // 保持原始
    }
    return content;
  }, [request, maskSensitive]);

  if (!request) {
    return (
      <div className="flex flex-col items-center justify-center h-full border border-border/70 rounded-xl bg-card p-8 text-center text-xs text-muted-foreground">
        从左侧列表选择一条请求以检视 Headers、Body 与重放调试
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full border border-border/70 rounded-xl bg-card overflow-hidden">
      {/* 顶部概览栏 */}
      <div className="flex items-center justify-between px-4 py-2.5 border-b border-border/50 bg-muted/20 text-xs">
        <div className="flex items-center gap-2 flex-wrap">
          <span className="font-mono font-bold text-foreground">{request.method}</span>
          <span className="font-mono text-muted-foreground">{request.path}</span>
          <span className="text-muted-foreground">|</span>
          <span className="text-muted-foreground">IP: {request.clientIp}</span>
          {request.isTruncated && (
            <span className="px-1.5 py-0.5 rounded bg-amber-500/10 text-amber-600 border border-amber-500/20 text-[10px]">
              Payload 已截断 (超 2MB)
            </span>
          )}
        </div>

        {request.detectedSignature && (
          <div className="flex items-center gap-1 text-[11px] text-purple-600 dark:text-purple-400 font-medium">
            <ShieldCheck className="w-3.5 h-3.5" />
            {request.detectedSignature.provider} 验签头已就绪
          </div>
        )}
      </div>

      {/* 选项卡导航 */}
      <div className="flex items-center justify-between px-4 border-b border-border/40 bg-muted/10">
        <div className="flex gap-4 text-xs">
          <button
            type="button"
            onClick={() => setActiveTab("body")}
            className={`py-2 border-b-2 font-medium transition-colors ${
              activeTab === "body"
                ? "border-primary text-primary"
                : "border-transparent text-muted-foreground hover:text-foreground"
            }`}
          >
            Payload / Body ({request.bodyFormat})
          </button>
          <button
            type="button"
            onClick={() => setActiveTab("headers")}
            className={`py-2 border-b-2 font-medium transition-colors ${
              activeTab === "headers"
                ? "border-primary text-primary"
                : "border-transparent text-muted-foreground hover:text-foreground"
            }`}
          >
            Headers ({Object.keys(request.headers).length})
          </button>
          <button
            type="button"
            onClick={() => setActiveTab("query")}
            className={`py-2 border-b-2 font-medium transition-colors ${
              activeTab === "query"
                ? "border-primary text-primary"
                : "border-transparent text-muted-foreground hover:text-foreground"
            }`}
          >
            Query Params ({Object.keys(request.queryParams).length})
          </button>
        </div>

        <div className="flex items-center gap-2">
          {activeTab === "body" && (
            <>
              <button
                type="button"
                onClick={() => setMaskSensitive(!maskSensitive)}
                className="text-[11px] text-muted-foreground hover:text-foreground flex items-center gap-1 py-1"
                title="切换敏感凭据脱敏"
              >
                {maskSensitive ? <EyeOff className="w-3.5 h-3.5" /> : <Eye className="w-3.5 h-3.5" />}
                {maskSensitive ? "已脱敏" : "脱敏预览"}
              </button>
              <button
                type="button"
                onClick={() => copyBody(displayedBody)}
                className="text-[11px] text-muted-foreground hover:text-foreground flex items-center gap-1 py-1"
              >
                {copiedBody ? <Check className="w-3.5 h-3.5 text-emerald-500" /> : <Copy className="w-3.5 h-3.5" />}
                复制
              </button>
            </>
          )}
          {activeTab === "headers" && (
            <button
              type="button"
              onClick={() => copyHeaders(JSON.stringify(request.headers, null, 2))}
              className="text-[11px] text-muted-foreground hover:text-foreground flex items-center gap-1 py-1"
            >
              {copiedHeaders ? <Check className="w-3.5 h-3.5 text-emerald-500" /> : <Copy className="w-3.5 h-3.5" />}
              复制 Headers
            </button>
          )}
        </div>
      </div>

      {/* 检视主体内容 */}
      <div className="flex-1 overflow-y-auto p-4 font-mono text-xs">
        {activeTab === "body" && (
          <pre className="whitespace-pre-wrap break-all leading-relaxed bg-muted/20 p-3 rounded-lg border border-border/40 select-text">
            {displayedBody || "<Empty Body>"}
          </pre>
        )}

        {activeTab === "headers" && (
          <div className="border border-border/50 rounded-lg overflow-hidden">
            <table className="w-full text-left text-xs divide-y divide-border/30">
              <thead className="bg-muted/30 text-muted-foreground text-[11px]">
                <tr>
                  <th className="px-3 py-1.5 w-1/3">Header Key</th>
                  <th className="px-3 py-1.5">Header Value</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-border/20">
                {Object.entries(request.headers).map(([k, v]) => (
                  <tr key={k} className="hover:bg-muted/20">
                    <td className="px-3 py-1.5 font-bold text-foreground truncate max-w-[150px]">
                      {k}
                    </td>
                    <td className="px-3 py-1.5 text-muted-foreground break-all">
                      {v}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}

        {activeTab === "query" && (
          <div className="border border-border/50 rounded-lg overflow-hidden">
            <table className="w-full text-left text-xs divide-y divide-border/30">
              <thead className="bg-muted/30 text-muted-foreground text-[11px]">
                <tr>
                  <th className="px-3 py-1.5 w-1/3">Param Key</th>
                  <th className="px-3 py-1.5">Param Value</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-border/20">
                {Object.keys(request.queryParams).length === 0 ? (
                  <tr>
                    <td colSpan={2} className="px-3 py-3 text-center text-muted-foreground">
                      无 Query 参数
                    </td>
                  </tr>
                ) : (
                  Object.entries(request.queryParams).map(([k, v]) => (
                    <tr key={k} className="hover:bg-muted/20">
                      <td className="px-3 py-1.5 font-bold text-foreground">{k}</td>
                      <td className="px-3 py-1.5 text-muted-foreground break-all">{v}</td>
                    </tr>
                  ))
                )}
              </tbody>
            </table>
          </div>
        )}
      </div>

      {/* 底部重放工作区 */}
      <div className="p-3 border-t border-border/40 bg-muted/10 space-y-2 text-xs">
        <div className="flex items-center gap-2">
          <span className="shrink-0 font-medium text-muted-foreground">重放目标:</span>
          <input
            type="text"
            value={replayUrl}
            onChange={(e) => setReplayUrl(e.target.value)}
            placeholder="http://localhost:3000/api/webhook"
            className="flex-1 rounded-md border border-input bg-background px-2.5 py-1 font-mono text-xs text-foreground focus:outline-none focus:ring-1 focus:ring-ring"
          />
          <button
            type="button"
            disabled={isReplaying}
            onClick={() => onInstantReplay(replayUrl)}
            className="inline-flex items-center gap-1 rounded-md bg-primary px-3 py-1 font-medium text-primary-foreground hover:bg-primary/90 transition-colors disabled:opacity-50"
          >
            <Play className="w-3 h-3" />
            原样重放
          </button>
          <button
            type="button"
            onClick={onOpenReplayDrawer}
            className="rounded-md border border-border px-3 py-1 text-muted-foreground hover:bg-accent hover:text-foreground transition-colors"
          >
            改包重放...
          </button>
        </div>

        {replayResult && (
          <div className="rounded-md border border-border/60 bg-card p-2 text-[11px] font-mono flex items-center justify-between">
            <div className="flex items-center gap-2">
              <span
                className={`font-bold px-1.5 py-0.5 rounded text-[10px] ${
                  replayResult.statusCode >= 200 && replayResult.statusCode < 300
                    ? "bg-emerald-500/10 text-emerald-600"
                    : "bg-rose-500/10 text-rose-600"
                }`}
              >
                {replayResult.statusCode} OK
              </span>
              <span className="text-muted-foreground">耗时: {replayResult.durationMs}ms</span>
            </div>
            <span className="truncate max-w-[280px] text-muted-foreground">
              {replayResult.responseBody}
            </span>
          </div>
        )}
      </div>
    </div>
  );
}
