import { useCallback, useEffect, useState } from "react";
import { Copy, Plus, Trash2, Webhook } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { ToolShell } from "@/components/toolbox/ToolShell";
import { useToast } from "@/components/ToastProvider";
import { useCopyToClipboard } from "@/toolbox/useCopyToClipboard";
import { isToolboxInvokeAvailable } from "@/toolbox/invoke";
import { RequestStreamList } from "./RequestStreamList";
import { RequestInspector } from "./RequestInspector";
import { ReplayDrawer } from "./ReplayDrawer";
import type {
  CapturedWebhookRequest,
  ReplayRequest,
  ReplayResult,
  WebhookEndpoint,
  WebhookServerStatus,
} from "./types";

export function WebhookCatcherTool() {
  const { pushToast } = useToast();

  const [endpoints, setEndpoints] = useState<WebhookEndpoint[]>([]);
  const [selectedEndpointId, setSelectedEndpointId] = useState<string>("");
  const [serverStatus, setServerStatus] = useState<WebhookServerStatus>({
    isRunning: false,
    port: 17692,
    activeEndpointsCount: 0,
    capturedRequestsCount: 0,
  });

  const [requests, setRequests] = useState<CapturedWebhookRequest[]>([]);
  const [selectedRequest, setSelectedRequest] = useState<CapturedWebhookRequest | null>(null);

  // 状态与错误
  const [error, setError] = useState<string | null>(null);

  // 弹窗
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [newEndpointOpen, setNewEndpointOpen] = useState(false);

  // 新建端点表单状态
  const [newEpName, setNewEpName] = useState("");
  const [newEpPath, setNewEpPath] = useState("/hook/my-service");
  const [newEpStatus, setNewEpStatus] = useState(200);
  const [newEpBody, setNewEpBody] = useState("{\"success\":true}");
  const [forwardEnabled, setForwardEnabled] = useState(false);
  const [forwardTarget, setForwardTarget] = useState("http://localhost:3000/api/webhook");

  // 重放状态
  const [isReplaying, setIsReplaying] = useState(false);
  const [replayResult, setReplayResult] = useState<ReplayResult | null>(null);

  const { copy: copyUrl } = useCopyToClipboard({
    onSuccess: () =>
      pushToast({
        title: "已复制本地 Webhook URL",
        kind: "success",
      }),
  });

  const fetchStatusAndEndpoints = useCallback(async () => {
    if (!isToolboxInvokeAvailable()) return;
    try {
      const status = await invoke<WebhookServerStatus>("webhook_get_server_status");
      if (status) setServerStatus(status);

      const eps = await invoke<WebhookEndpoint[]>("webhook_list_endpoints");
      if (Array.isArray(eps)) {
        setEndpoints(eps);
        if (eps.length > 0 && !selectedEndpointId) {
          setSelectedEndpointId(eps[0].id);
        }
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  }, [selectedEndpointId]);

  const fetchRequests = useCallback(async () => {
    if (!isToolboxInvokeAvailable()) return;
    try {
      const reqs = await invoke<CapturedWebhookRequest[]>("webhook_list_requests", {
        endpointId: selectedEndpointId || undefined,
        limit: 100,
      });
      if (Array.isArray(reqs)) {
        setRequests(reqs);
        // 如果当前选中的请求不在列表里，选中第一条
        if (reqs.length > 0 && (!selectedRequest || !reqs.some((r) => r.id === selectedRequest.id))) {
          setSelectedRequest(reqs[0]);
        }
      }
    } catch (err) {
      console.error(err);
    }
  }, [selectedEndpointId, selectedRequest]);

  useEffect(() => {
    void fetchStatusAndEndpoints();
    // 自动启动服务（如果未运行）
    void invoke("webhook_start_server").then(() => {
      void fetchStatusAndEndpoints();
    });
  }, [fetchStatusAndEndpoints]);

  useEffect(() => {
    void fetchRequests();
    const interval = setInterval(() => {
      void fetchRequests();
    }, 2500);
    return () => clearInterval(interval);
  }, [fetchRequests]);

  const activeEndpoint = endpoints.find((e) => e.id === selectedEndpointId) || endpoints[0];
  const localWebhookUrl = activeEndpoint
    ? `http://127.0.0.1:${serverStatus.port}${activeEndpoint.path.startsWith("/") ? "" : "/"}${activeEndpoint.path}`
    : `http://127.0.0.1:${serverStatus.port}/hook/default`;

  const handleCreateEndpoint = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!newEpName.trim() || !newEpPath.trim()) return;

    try {
      const saved = await invoke<WebhookEndpoint>("webhook_save_endpoint", {
        endpoint: {
          id: "",
          name: newEpName.trim(),
          path: newEpPath.trim(),
          isActive: true,
          customResponse: {
            statusCode: Number(newEpStatus),
            contentType: "application/json",
            body: newEpBody,
            delayMs: 0,
          },
          forwardConfig: forwardEnabled
            ? {
                enabled: true,
                targetUrl: forwardTarget.trim(),
              }
            : undefined,
          createdAt: 0,
        },
      });

      pushToast({
        title: "已创建 Webhook 端点",
        kind: "success",
      });
      setNewEndpointOpen(false);
      setNewEpName("");
      void fetchStatusAndEndpoints();
      setSelectedEndpointId(saved.id);
    } catch (err) {
      pushToast({
        title: "创建失败",
        description: String(err),
        kind: "error",
      });
    }
  };

  const handleInstantReplay = async (targetUrl: string): Promise<ReplayResult> => {
    if (!selectedRequest) throw new Error("No request selected");
    setIsReplaying(true);
    try {
      const res = await invoke<ReplayResult>("webhook_execute_replay", {
        param: {
          requestId: selectedRequest.id,
          targetUrl,
        },
      });
      setReplayResult(res);
      pushToast({
        title: `重放成功 (HTTP ${res.statusCode})`,
        description: `耗时 ${res.durationMs}ms`,
        kind: "success",
      });
      return res;
    } catch (err) {
      pushToast({
        title: "重放失败",
        description: String(err),
        kind: "error",
      });
      throw err;
    } finally {
      setIsReplaying(false);
    }
  };

  const handleExecuteReplayFromDrawer = async (req: ReplayRequest): Promise<ReplayResult> => {
    const res = await invoke<ReplayResult>("webhook_execute_replay", { param: req });
    setReplayResult(res);
    pushToast({
      title: `重放成功 (HTTP ${res.statusCode})`,
      description: `耗时 ${res.durationMs}ms`,
      kind: "success",
    });
    return res;
  };

  const handleClear = async () => {
    try {
      await invoke("webhook_clear_requests", {
        endpointId: selectedEndpointId || undefined,
      });
      setRequests([]);
      setSelectedRequest(null);
      pushToast({
        title: "已清空请求流水",
        kind: "success",
      });
    } catch (err) {
      pushToast({
        title: "清空失败",
        description: String(err),
        kind: "error",
      });
    }
  };

  const handleDeleteEndpoint = async (id: string) => {
    if (!window.confirm("确定要删除此端点吗？")) return;
    try {
      await invoke("webhook_delete_endpoint", { id });
      pushToast({
        title: "端点已删除",
        kind: "success",
      });
      void fetchStatusAndEndpoints();
    } catch (err) {
      pushToast({
        title: "删除失败",
        description: String(err),
        kind: "error",
      });
    }
  };

  return (
    <ToolShell
      title="Webhook 捕获与重放"
      description="本地私密接收第三方回调，Payload与签名深度检视，一键重放到本地开发端口"
      icon={
        <div className="flex h-10 w-10 items-center justify-center rounded-xl bg-indigo-500/10 text-indigo-600">
          <Webhook className="h-5 w-5" />
        </div>
      }
      error={error}
      actions={
        <div className="flex items-center gap-2">
          <div className="flex items-center gap-1.5 px-2.5 py-1 rounded-lg border border-border bg-card text-xs font-mono text-muted-foreground">
            <span
              className={`inline-block w-2 h-2 rounded-full ${
                serverStatus.isRunning ? "bg-emerald-500" : "bg-muted"
              }`}
            />
            <span>:{serverStatus.port}</span>
          </div>
          <button
            type="button"
            onClick={() => {
              setNewEndpointOpen(true);
            }}
            className="inline-flex items-center gap-1.5 rounded-lg bg-primary px-3 py-1.5 text-xs font-medium text-primary-foreground hover:bg-primary/90 transition-colors shadow-sm"
          >
            <Plus className="w-3.5 h-3.5" />
            新建端点
          </button>
        </div>
      }
    >
      <div className="space-y-3">
        {/* 端点控制条 */}
        <div className="flex items-center justify-between gap-3 p-3 rounded-xl border border-border/70 bg-card flex-wrap">
          <div className="flex items-center gap-3 flex-1 min-w-[280px]">
            <select
              value={selectedEndpointId}
              onChange={(e) => setSelectedEndpointId(e.target.value)}
              className="rounded-lg border border-input bg-background px-3 py-1.5 text-xs text-foreground focus:outline-none focus:ring-1 focus:ring-ring font-medium"
            >
              {endpoints.length === 0 && <option value="">默认端点 (/hook/default)</option>}
              {endpoints.map((ep) => (
                <option key={ep.id} value={ep.id}>
                  {ep.name} ({ep.path})
                </option>
              ))}
            </select>

            <div className="flex items-center gap-1.5 bg-muted/40 border border-border/50 px-2.5 py-1 rounded-lg text-xs font-mono text-muted-foreground truncate">
              <span className="truncate">{localWebhookUrl}</span>
              <button
                type="button"
                onClick={() => copyUrl(localWebhookUrl)}
                className="text-muted-foreground hover:text-foreground p-0.5 ml-1"
                title="复制端点 URL"
              >
                <Copy className="w-3.5 h-3.5" />
              </button>
            </div>
          </div>

          <div className="flex items-center gap-2">
            {activeEndpoint && (
              <button
                type="button"
                onClick={() => handleDeleteEndpoint(activeEndpoint.id)}
                className="text-xs text-muted-foreground hover:text-destructive p-1.5 rounded border border-border/50 hover:bg-destructive/10 transition-colors"
                title="删除当前端点"
              >
                <Trash2 className="w-3.5 h-3.5" />
              </button>
            )}
          </div>
        </div>

        {/* 经典双栏调试工作区 */}
        <div className="grid grid-cols-1 md:grid-cols-12 gap-3 h-[580px]">
          {/* 左侧流水 */}
          <div className="md:col-span-4 h-full">
            <RequestStreamList
              requests={requests}
              selectedId={selectedRequest?.id || null}
              onSelect={(r) => setSelectedRequest(r)}
              onClear={handleClear}
            />
          </div>

          {/* 右侧检视面板 */}
          <div className="md:col-span-8 h-full">
            <RequestInspector
              request={selectedRequest}
              onInstantReplay={handleInstantReplay}
              onOpenReplayDrawer={() => setDrawerOpen(true)}
              isReplaying={isReplaying}
              replayResult={replayResult}
            />
          </div>
        </div>
      </div>

      {/* 改包重放弹窗 */}
      <ReplayDrawer
        request={selectedRequest}
        isOpen={drawerOpen}
        onClose={() => setDrawerOpen(false)}
        onExecuteReplay={handleExecuteReplayFromDrawer}
      />

      {/* 新建端点弹窗 */}
      {newEndpointOpen && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm p-4 animate-in fade-in-0 duration-150">
          <div className="relative w-full max-w-md rounded-xl border border-border bg-card p-6 shadow-xl space-y-4 text-xs">
            <h3 className="text-base font-bold text-foreground">新建 Webhook 端点</h3>
            <form onSubmit={handleCreateEndpoint} className="space-y-3">
              <div>
                <label className="block text-muted-foreground mb-1 font-medium">端点名称</label>
                <input
                  type="text"
                  required
                  value={newEpName}
                  onChange={(e) => setNewEpName(e.target.value)}
                  placeholder="如: GitHub Push 回调"
                  className="w-full rounded-md border border-input bg-background px-3 py-1.5 text-xs text-foreground focus:outline-none focus:ring-1 focus:ring-ring"
                />
              </div>

              <div>
                <label className="block text-muted-foreground mb-1 font-medium">路径 (以 / 开头)</label>
                <input
                  type="text"
                  required
                  value={newEpPath}
                  onChange={(e) => setNewEpPath(e.target.value)}
                  placeholder="/hook/github"
                  className="w-full font-mono rounded-md border border-input bg-background px-3 py-1.5 text-xs text-foreground focus:outline-none focus:ring-1 focus:ring-ring"
                />
              </div>

              <div className="grid grid-cols-2 gap-2">
                <div>
                  <label className="block text-muted-foreground mb-1 font-medium">Mock 响应状态</label>
                  <input
                    type="number"
                    value={newEpStatus}
                    onChange={(e) => setNewEpStatus(Number(e.target.value))}
                    className="w-full font-mono rounded-md border border-input bg-background px-3 py-1.5 text-xs text-foreground focus:outline-none focus:ring-1 focus:ring-ring"
                  />
                </div>
                <div>
                  <label className="block text-muted-foreground mb-1 font-medium">Mock Body</label>
                  <input
                    type="text"
                    value={newEpBody}
                    onChange={(e) => setNewEpBody(e.target.value)}
                    className="w-full font-mono rounded-md border border-input bg-background px-3 py-1.5 text-xs text-foreground focus:outline-none focus:ring-1 focus:ring-ring"
                  />
                </div>
              </div>

              <div className="space-y-1.5 pt-1 border-t border-border/40">
                <div className="flex items-center gap-2">
                  <input
                    type="checkbox"
                    id="forwardRelay"
                    checked={forwardEnabled}
                    onChange={(e) => setForwardEnabled(e.target.checked)}
                    className="rounded border-border text-primary focus:ring-primary"
                  />
                  <label htmlFor="forwardRelay" className="text-muted-foreground select-none">
                    开启实时透传 (收到请求时自动镜像转发给本地服务)
                  </label>
                </div>
                {forwardEnabled && (
                  <input
                    type="text"
                    value={forwardTarget}
                    onChange={(e) => setForwardTarget(e.target.value)}
                    placeholder="http://localhost:3000/api/webhook"
                    className="w-full font-mono rounded-md border border-input bg-background px-3 py-1.5 text-xs text-foreground focus:outline-none focus:ring-1 focus:ring-ring"
                  />
                )}
              </div>

              <div className="flex items-center justify-end gap-2 pt-3 border-t border-border/40">
                <button
                  type="button"
                  onClick={() => setNewEndpointOpen(false)}
                  className="rounded-md border border-border px-3 py-1.5 text-muted-foreground hover:bg-accent transition-colors"
                >
                  取消
                </button>
                <button
                  type="submit"
                  className="rounded-md bg-primary px-4 py-1.5 font-medium text-primary-foreground hover:bg-primary/90 transition-colors"
                >
                  创建端点
                </button>
              </div>
            </form>
          </div>
        </div>
      )}
    </ToolShell>
  );
}
