import { useState } from "react";
import { Loader2, Play, X } from "lucide-react";
import type { MonitorTarget, ProbeTestResult, ProbeType } from "./types";

interface MonitorFormModalProps {
  initialTarget?: MonitorTarget | null;
  isOpen: boolean;
  onClose: () => void;
  onSave: (target: MonitorTarget) => Promise<void>;
  onTestProbe: (target: MonitorTarget) => Promise<ProbeTestResult>;
}

export function MonitorFormModal({
  initialTarget,
  isOpen,
  onClose,
  onSave,
  onTestProbe,
}: MonitorFormModalProps) {
  const [probeType, setProbeType] = useState<ProbeType>(
    initialTarget?.probeType || "http",
  );
  const [name, setName] = useState(initialTarget?.name || "");
  const [target, setTarget] = useState(initialTarget?.target || "https://");
  const [intervalSec, setIntervalSec] = useState(
    initialTarget?.intervalSec || 30,
  );
  const [timeoutMs, setTimeoutMs] = useState(initialTarget?.timeoutMs || 5000);
  const [retryCount, setRetryCount] = useState(initialTarget?.retryCount || 2);
  const [tagsInput, setTagsInput] = useState(
    initialTarget?.tags ? initialTarget.tags.join(", ") : "",
  );

  // HTTP Options
  const [httpMethod, setHttpMethod] = useState(
    initialTarget?.httpOptions?.method || "GET",
  );
  const [expectedStatus, setExpectedStatus] = useState(
    initialTarget?.httpOptions?.expectedStatus
      ? initialTarget.httpOptions.expectedStatus.join(", ")
      : "200, 201, 204",
  );
  const [keyword, setKeyword] = useState(
    initialTarget?.httpOptions?.keyword || "",
  );
  const [ignoreSslErrors, setIgnoreSslErrors] = useState(
    initialTarget?.httpOptions?.ignoreSslErrors || false,
  );

  // Alert Config
  const [systemNotification, setSystemNotification] = useState(
    initialTarget?.alertConfig?.systemNotification ?? true,
  );
  const [webhookUrl, setWebhookUrl] = useState(
    initialTarget?.alertConfig?.webhookUrl || "",
  );

  // Test probe state
  const [testing, setTesting] = useState(false);
  const [testResult, setTestResult] = useState<ProbeTestResult | null>(null);
  const [saving, setSaving] = useState(false);

  if (!isOpen) return null;

  const buildTargetModel = (): MonitorTarget => {
    const parsedStatuses = expectedStatus
      .split(",")
      .map((s) => parseInt(s.trim(), 10))
      .filter((n) => !isNaN(n));

    const tags = tagsInput
      .split(",")
      .map((t) => t.trim())
      .filter(Boolean);

    return {
      id: initialTarget?.id || "",
      name: name.trim() || "新监控项",
      probeType,
      target: target.trim(),
      intervalSec: Number(intervalSec),
      timeoutMs: Number(timeoutMs),
      retryCount: Number(retryCount),
      recoverCount: 1,
      isEnabled: initialTarget?.isEnabled ?? true,
      tags,
      httpOptions:
        probeType === "http"
          ? {
              method: httpMethod,
              headers: initialTarget?.httpOptions?.headers || {},
              expectedStatus:
                parsedStatuses.length > 0 ? parsedStatuses : [200, 201, 204],
              keyword: keyword.trim() || undefined,
              ignoreSslErrors,
              warnSslDaysRemaining: 14,
            }
          : undefined,
      tcpOptions:
        probeType === "tcp"
          ? {
              host: target.split(":")[0] || target,
              port: parseInt(target.split(":")[1] || "80", 10),
            }
          : undefined,
      dnsOptions:
        probeType === "dns"
          ? {
              recordType: "A",
            }
          : undefined,
      alertConfig: {
        systemNotification,
        soundAlert: false,
        webhookUrl: webhookUrl.trim() || undefined,
      },
      createdAt: initialTarget?.createdAt || 0,
      updatedAt: initialTarget?.updatedAt || 0,
    };
  };

  const handleRunTest = async () => {
    const model = buildTargetModel();
    setTesting(true);
    setTestResult(null);
    try {
      const res = await onTestProbe(model);
      setTestResult(res);
    } catch (err) {
      setTestResult({
        success: false,
        status: "down",
        responseTimeMs: 0,
        message: err instanceof Error ? err.message : String(err),
      });
    } finally {
      setTesting(false);
    }
  };

  const handleFormSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!name.trim() || !target.trim()) return;
    setSaving(true);
    try {
      await onSave(buildTargetModel());
      onClose();
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm p-4 animate-in fade-in-0 duration-150">
      <div className="relative w-full max-w-lg rounded-xl border border-border bg-card p-6 shadow-xl space-y-4 max-h-[90vh] overflow-y-auto">
        <div className="flex items-center justify-between border-b border-border/40 pb-3">
          <h2 className="text-lg font-bold">
            {initialTarget ? "编辑监控项" : "新建监控目标"}
          </h2>
          <button
            type="button"
            onClick={onClose}
            className="rounded-md p-1 text-muted-foreground hover:bg-muted hover:text-foreground"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        <form onSubmit={handleFormSubmit} className="space-y-4 text-xs">
          {/* 监控类型选择 */}
          <div>
            <label className="block text-muted-foreground mb-1 font-medium">
              探针协议类型
            </label>
            <div className="grid grid-cols-4 gap-2">
              {(["http", "tcp", "ping", "dns"] as const).map((t) => (
                <button
                  key={t}
                  type="button"
                  onClick={() => {
                    setProbeType(t);
                    if (t === "http" && !target.startsWith("http")) {
                      setTarget("https://");
                    } else if (t === "tcp" && target.startsWith("http")) {
                      setTarget("127.0.0.1:80");
                    }
                  }}
                  className={`rounded-lg border py-1.5 px-3 font-semibold uppercase text-center transition-colors ${
                    probeType === t
                      ? "border-primary bg-primary/10 text-primary"
                      : "border-border bg-muted/40 text-muted-foreground hover:bg-muted"
                  }`}
                >
                  {t}
                </button>
              ))}
            </div>
          </div>

          {/* 名称与目标 */}
          <div className="space-y-3">
            <div>
              <label className="block text-muted-foreground mb-1 font-medium">
                监控名称
              </label>
              <input
                type="text"
                required
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder="如：生产环境 API 网关"
                className="w-full rounded-md border border-input bg-background px-3 py-1.5 text-xs text-foreground focus:outline-none focus:ring-1 focus:ring-ring"
              />
            </div>

            <div>
              <label className="block text-muted-foreground mb-1 font-medium">
                {probeType === "http"
                  ? "目标 URL"
                  : probeType === "tcp"
                    ? "目标主机与端口 (Host:Port)"
                    : "目标主机或域名"}
              </label>
              <input
                type="text"
                required
                value={target}
                onChange={(e) => setTarget(e.target.value)}
                placeholder={
                  probeType === "http"
                    ? "https://api.example.com/health"
                    : probeType === "tcp"
                      ? "1.2.3.4:3306"
                      : "example.com"
                }
                className="w-full font-mono rounded-md border border-input bg-background px-3 py-1.5 text-xs text-foreground focus:outline-none focus:ring-1 focus:ring-ring"
              />
            </div>
          </div>

          {/* 探测周期与防抖 */}
          <div className="grid grid-cols-3 gap-3">
            <div>
              <label className="block text-muted-foreground mb-1 font-medium">
                检测频率
              </label>
              <select
                value={intervalSec}
                onChange={(e) => setIntervalSec(Number(e.target.value))}
                className="w-full rounded-md border border-input bg-background px-2.5 py-1.5 text-xs text-foreground focus:outline-none focus:ring-1 focus:ring-ring"
              >
                <option value={10}>每 10 秒</option>
                <option value={30}>每 30 秒</option>
                <option value={60}>每 1 分钟</option>
                <option value={300}>每 5 分钟</option>
              </select>
            </div>

            <div>
              <label className="block text-muted-foreground mb-1 font-medium">
                超时时间 (ms)
              </label>
              <input
                type="number"
                min={500}
                max={30000}
                step={500}
                value={timeoutMs}
                onChange={(e) => setTimeoutMs(Number(e.target.value))}
                className="w-full font-mono rounded-md border border-input bg-background px-2.5 py-1.5 text-xs text-foreground focus:outline-none focus:ring-1 focus:ring-ring"
              />
            </div>

            <div>
              <label className="block text-muted-foreground mb-1 font-medium">
                连续失败告警
              </label>
              <select
                value={retryCount}
                onChange={(e) => setRetryCount(Number(e.target.value))}
                className="w-full rounded-md border border-input bg-background px-2.5 py-1.5 text-xs text-foreground focus:outline-none focus:ring-1 focus:ring-ring"
              >
                <option value={1}>1 次立即报警</option>
                <option value={2}>连续 2 次失败</option>
                <option value={3}>连续 3 次失败</option>
              </select>
            </div>
          </div>

          {/* HTTP 专属配置 */}
          {probeType === "http" && (
            <div className="rounded-lg border border-border/50 bg-muted/20 p-3 space-y-3">
              <span className="font-semibold block text-[11px] text-muted-foreground">
                HTTP 高级参数
              </span>
              <div className="grid grid-cols-2 gap-3">
                <div>
                  <label className="block text-muted-foreground mb-1 font-medium">
                    请求方法
                  </label>
                  <select
                    value={httpMethod}
                    onChange={(e) => setHttpMethod(e.target.value)}
                    className="w-full rounded-md border border-input bg-background px-2.5 py-1.5 text-xs text-foreground focus:outline-none focus:ring-1 focus:ring-ring"
                  >
                    <option value="GET">GET</option>
                    <option value="POST">POST</option>
                    <option value="HEAD">HEAD</option>
                  </select>
                </div>
                <div>
                  <label className="block text-muted-foreground mb-1 font-medium">
                    期望状态码 (逗号隔开)
                  </label>
                  <input
                    type="text"
                    value={expectedStatus}
                    onChange={(e) => setExpectedStatus(e.target.value)}
                    placeholder="200, 201, 204"
                    className="w-full font-mono rounded-md border border-input bg-background px-2.5 py-1.5 text-xs text-foreground focus:outline-none focus:ring-1 focus:ring-ring"
                  />
                </div>
              </div>

              <div>
                <label className="block text-muted-foreground mb-1 font-medium">
                  响应包含关键词 (可选)
                </label>
                <input
                  type="text"
                  value={keyword}
                  onChange={(e) => setKeyword(e.target.value)}
                  placeholder="如: success 或 ok"
                  className="w-full font-mono rounded-md border border-input bg-background px-2.5 py-1.5 text-xs text-foreground focus:outline-none focus:ring-1 focus:ring-ring"
                />
              </div>

              <div className="flex items-center gap-2 pt-1">
                <input
                  type="checkbox"
                  id="ignoreSsl"
                  checked={ignoreSslErrors}
                  onChange={(e) => setIgnoreSslErrors(e.target.checked)}
                  className="rounded border-border text-primary focus:ring-primary"
                />
                <label htmlFor="ignoreSsl" className="text-muted-foreground select-none">
                  忽略自签名或证书域名不匹配错误
                </label>
              </div>
            </div>
          )}

          {/* 告警与通知 */}
          <div className="space-y-2">
            <label className="block text-muted-foreground font-medium">
              告警通道
            </label>
            <div className="flex items-center gap-2">
              <input
                type="checkbox"
                id="sysNotify"
                checked={systemNotification}
                onChange={(e) => setSystemNotification(e.target.checked)}
                className="rounded border-border text-primary focus:ring-primary"
              />
              <label htmlFor="sysNotify" className="text-muted-foreground select-none">
                桌面系统通知与消息中心
              </label>
            </div>
            <div>
              <input
                type="url"
                value={webhookUrl}
                onChange={(e) => setWebhookUrl(e.target.value)}
                placeholder="飞书 / 钉钉 / 自定义 Webhook URL"
                className="w-full font-mono rounded-md border border-input bg-background px-3 py-1.5 text-xs text-foreground focus:outline-none focus:ring-1 focus:ring-ring"
              />
            </div>
          </div>

          {/* 标签 */}
          <div>
            <label className="block text-muted-foreground mb-1 font-medium">
              分类标签 (用逗号隔开)
            </label>
            <input
              type="text"
              value={tagsInput}
              onChange={(e) => setTagsInput(e.target.value)}
              placeholder="Prod, API, Gateway"
              className="w-full rounded-md border border-input bg-background px-3 py-1.5 text-xs text-foreground focus:outline-none focus:ring-1 focus:ring-ring"
            />
          </div>

          {/* 测试连接反馈 */}
          {testResult && (
            <div
              className={`rounded-lg border px-3 py-2 text-xs flex items-center justify-between ${
                testResult.success
                  ? "border-emerald-500/30 bg-emerald-500/10 text-emerald-600 dark:text-emerald-400"
                  : "border-rose-500/30 bg-rose-500/10 text-rose-600 dark:text-rose-400"
              }`}
            >
              <span>{testResult.message}</span>
              <span className="font-mono font-semibold">{testResult.responseTimeMs}ms</span>
            </div>
          )}

          {/* 底部按钮栏 */}
          <div className="flex items-center justify-between pt-3 border-t border-border/40">
            <button
              type="button"
              disabled={testing}
              onClick={handleRunTest}
              className="inline-flex items-center gap-1.5 rounded-md border border-border px-3 py-1.5 font-medium text-muted-foreground hover:bg-accent hover:text-foreground transition-colors disabled:opacity-50"
            >
              {testing ? (
                <Loader2 className="w-3.5 h-3.5 animate-spin" />
              ) : (
                <Play className="w-3.5 h-3.5" />
              )}
              测试连接
            </button>

            <div className="flex items-center gap-2">
              <button
                type="button"
                onClick={onClose}
                className="rounded-md border border-border px-3 py-1.5 text-muted-foreground hover:bg-accent transition-colors"
              >
                取消
              </button>
              <button
                type="submit"
                disabled={saving}
                className="inline-flex items-center gap-1 rounded-md bg-primary px-4 py-1.5 font-medium text-primary-foreground hover:bg-primary/90 transition-colors disabled:opacity-50"
              >
                {saving && <Loader2 className="w-3.5 h-3.5 animate-spin" />}
                保存并启用
              </button>
            </div>
          </div>
        </form>
      </div>
    </div>
  );
}
