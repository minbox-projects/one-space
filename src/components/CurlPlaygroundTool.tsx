import { useMemo, useState } from "react";
import {
  Check,
  Clock,
  Copy,
  Loader2,
  Plus,
  RotateCcw,
  Send,
  Trash2,
} from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "react-i18next";
import { ToolShell } from "@/components/toolbox/ToolShell";
import { ToolStatusBadge } from "@/components/toolbox/ToolStatusBadge";
import { useToast } from "./ToastProvider";
import { getToolboxTool } from "@/toolbox/registry";
import { isToolboxInvokeAvailable } from "@/toolbox/invoke";
import { useCopyToClipboard } from "@/toolbox/useCopyToClipboard";
import {
  type CodeExportLanguage,
  type KeyValuePair,
  type ParsedHttpRequest,
  buildEffectiveUrl,
  generateCode,
  parseCurlCommand,
} from "@/lib/curlParser";

interface HttpResponsePayload {
  status: number;
  status_text: string;
  headers: Record<string, string>;
  body: string;
  elapsed_ms: number;
  is_truncated: boolean;
}

export function CurlPlaygroundTool() {
  const { t } = useTranslation();
  const { pushToast } = useToast();
  const toolDescriptor = getToolboxTool("curl-playground");
  const ToolIcon = toolDescriptor?.icon ?? Send;
  const iconClassName =
    toolDescriptor?.iconClassName ?? "bg-blue-500/10 text-blue-600";

  // 输入的 cURL 命令行
  const [curlInput, setCurlInput] = useState("");

  // 请求结构化状态
  const [method, setMethod] = useState("GET");
  const [baseUrl, setBaseUrl] = useState("");
  const [params, setParams] = useState<KeyValuePair[]>([]);
  const [headers, setHeaders] = useState<KeyValuePair[]>([]);
  const [body, setBody] = useState("");

  // Tabs
  const [requestTab, setRequestTab] = useState<"params" | "headers" | "body">(
    "headers",
  );
  const [bottomTab, setBottomTab] = useState<"response" | "export">("response");
  const [exportLang, setExportLang] =
    useState<CodeExportLanguage>("typescript-fetch");

  // 执行与响应状态
  const [loading, setLoading] = useState(false);
  const [response, setResponse] = useState<HttpResponsePayload | null>(null);
  const [requestError, setRequestError] = useState<string | null>(null);

  const { copied: copiedResponse, copy: copyResponse } = useCopyToClipboard({
    onSuccess: () =>
      pushToast({
        title: t("curlResponseCopied", "Response body copied"),
        kind: "success",
      }),
  });

  const { copied: copiedCode, copy: copyCode } = useCopyToClipboard({
    onSuccess: () =>
      pushToast({
        title: t("curlCodeCopied", "Exported code copied"),
        kind: "success",
      }),
  });

  // 组装当前有效 URL
  const effectiveUrl = useMemo(
    () => buildEffectiveUrl(baseUrl, params),
    [baseUrl, params],
  );

  // 构造结构化请求对象（用于代码导出）
  const parsedRequestModel: ParsedHttpRequest = useMemo(
    () => ({
      method,
      url: effectiveUrl,
      baseUrl,
      params,
      headers,
      body: body || undefined,
    }),
    [method, effectiveUrl, baseUrl, params, headers, body],
  );

  // 解析并导入 cURL
  const handleImportCurl = (curlText: string) => {
    if (!curlText.trim()) return;
    try {
      const parsed = parseCurlCommand(curlText);
      setMethod(parsed.method);
      setBaseUrl(parsed.baseUrl);
      setParams(parsed.params);
      setHeaders(parsed.headers);
      setBody(parsed.body || "");
      setRequestError(null);
    } catch (err) {
      setRequestError(
        err instanceof Error ? err.message : "Failed to parse cURL command",
      );
    }
  };

  // 发起请求
  const handleSendRequest = async () => {
    if (!baseUrl.trim()) return;
    setLoading(true);
    setRequestError(null);
    setResponse(null);
    setBottomTab("response");

    const activeHeadersMap: Record<string, string> = {};
    for (const h of headers) {
      if (h.enabled && h.name.trim()) {
        activeHeadersMap[h.name.trim()] = h.value;
      }
    }

    try {
      if (isToolboxInvokeAvailable()) {
        const res = await invoke<HttpResponsePayload>("send_http_request", {
          request: {
            method,
            url: effectiveUrl,
            headers: activeHeadersMap,
            body: method !== "GET" && method !== "HEAD" ? body : undefined,
          },
        });
        setResponse(res);
      } else {
        // 前端测试或离线 fallback 模拟
        const start = Date.now();
        const fetchRes = await fetch(effectiveUrl, {
          method,
          headers: activeHeadersMap,
          body:
            method !== "GET" && method !== "HEAD" && body ? body : undefined,
        });
        const elapsed = Date.now() - start;
        const text = await fetchRes.text();
        const resHeaders: Record<string, string> = {};
        fetchRes.headers.forEach((v, k) => {
          resHeaders[k] = v;
        });

        setResponse({
          status: fetchRes.status,
          status_text: fetchRes.statusText,
          headers: resHeaders,
          body: text,
          elapsed_ms: elapsed,
          is_truncated: false,
        });
      }
    } catch (err) {
      setRequestError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  };

  // 格式化 JSON Response
  const formattedResponseBody = useMemo(() => {
    if (!response?.body) return "";
    try {
      return JSON.stringify(JSON.parse(response.body), null, 2);
    } catch {
      return response.body;
    }
  }, [response]);

  // 生成的导出代码
  const generatedExportCode = useMemo(
    () => generateCode(parsedRequestModel, exportLang),
    [parsedRequestModel, exportLang],
  );

  return (
    <ToolShell
      titleId="curl-playground-title"
      title={t("curlPlayground", "cURL Playground & Client")}
      description={t(
        "curlPlaygroundDesc",
        "Debug, inspect and replay HTTP cURL requests natively without CORS limits.",
      )}
      icon={
        <div className={`rounded-lg p-2 ${iconClassName}`}>
          <ToolIcon className="h-5 w-5" />
        </div>
      }
      actions={
        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={() => {
              setCurlInput("");
              setBaseUrl("");
              setParams([]);
              setHeaders([]);
              setBody("");
              setResponse(null);
              setRequestError(null);
            }}
            className="inline-flex h-8 items-center gap-1.5 rounded-md border bg-background px-3 text-xs font-medium text-muted-foreground hover:bg-muted"
          >
            <RotateCcw className="h-3.5 w-3.5" />
            {t("curlClear", "Reset")}
          </button>
        </div>
      }
      error={requestError}
    >
      <div className="space-y-4">
        {/* cURL 粘贴导入区 */}
        <div className="rounded-lg border bg-card p-3 space-y-2">
          <div className="flex items-center justify-between text-xs font-medium text-muted-foreground">
            <span>{t("curlInputPrompt", "Paste cURL Command to Parse")}</span>
            {curlInput ? (
              <button
                type="button"
                onClick={() => handleImportCurl(curlInput)}
                className="text-primary hover:underline"
              >
                {t("curlReParse", "Re-parse")}
              </button>
            ) : null}
          </div>
          <div className="flex gap-2">
            <input
              type="text"
              value={curlInput}
              onChange={(e) => {
                setCurlInput(e.target.value);
                handleImportCurl(e.target.value);
              }}
              placeholder='curl -X POST "https://api.example.com" -H "..." -d "..."'
              className="h-9 flex-1 rounded-md border bg-background px-3 font-mono text-xs outline-none ring-offset-background focus-visible:ring-2 focus-visible:ring-ring"
            />
          </div>
        </div>

        {/* 请求配置栏：Method + URL + Send */}
        <div className="rounded-lg border bg-card p-3">
          <div className="flex flex-wrap items-center gap-2">
            <select
              value={method}
              onChange={(e) => setMethod(e.target.value)}
              className="h-9 rounded-md border bg-background px-3 text-xs font-semibold uppercase outline-none ring-offset-background focus-visible:ring-2 focus-visible:ring-ring"
            >
              {["GET", "POST", "PUT", "DELETE", "PATCH", "HEAD"].map((m) => (
                <option key={m} value={m}>
                  {m}
                </option>
              ))}
            </select>

            <input
              type="text"
              value={baseUrl}
              onChange={(e) => setBaseUrl(e.target.value)}
              placeholder="https://api.example.com/endpoint"
              className="h-9 min-w-72 flex-1 rounded-md border bg-background px-3 font-mono text-xs outline-none ring-offset-background focus-visible:ring-2 focus-visible:ring-ring"
            />

            <button
              type="button"
              onClick={handleSendRequest}
              disabled={loading || !baseUrl.trim()}
              className="inline-flex h-9 items-center gap-1.5 rounded-md bg-primary px-5 text-xs font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
            >
              {loading ? (
                <Loader2 className="h-3.5 w-3.5 animate-spin" />
              ) : (
                <Send className="h-3.5 w-3.5" />
              )}
              {t("curlSendRequest", "Send")}
            </button>
          </div>

          {/* 请求参数 Tabs */}
          <div className="mt-3 border-t pt-2">
            <div className="flex items-center gap-4 text-xs font-medium">
              <button
                type="button"
                onClick={() => setRequestTab("headers")}
                className={`pb-1 border-b-2 transition-colors ${
                  requestTab === "headers"
                    ? "border-primary text-foreground"
                    : "border-transparent text-muted-foreground hover:text-foreground"
                }`}
              >
                {t("curlTabHeaders", "Headers")} ({headers.length})
              </button>
              <button
                type="button"
                onClick={() => setRequestTab("params")}
                className={`pb-1 border-b-2 transition-colors ${
                  requestTab === "params"
                    ? "border-primary text-foreground"
                    : "border-transparent text-muted-foreground hover:text-foreground"
                }`}
              >
                {t("curlTabParams", "Query Params")} ({params.length})
              </button>
              <button
                type="button"
                onClick={() => setRequestTab("body")}
                className={`pb-1 border-b-2 transition-colors ${
                  requestTab === "body"
                    ? "border-primary text-foreground"
                    : "border-transparent text-muted-foreground hover:text-foreground"
                }`}
              >
                {t("curlTabBody", "Body")} {body ? "●" : ""}
              </button>
            </div>

            {/* Tab 内容 */}
            <div className="mt-3">
              {/* Headers Tab */}
              {requestTab === "headers" ? (
                <div className="space-y-2">
                  {headers.map((h, idx) => (
                    <div key={idx} className="flex items-center gap-2">
                      <input
                        type="checkbox"
                        checked={h.enabled}
                        onChange={(e) => {
                          const updated = [...headers];
                          updated[idx].enabled = e.target.checked;
                          setHeaders(updated);
                        }}
                        className="h-4 w-4 rounded border-gray-300 text-primary"
                      />
                      <input
                        type="text"
                        value={h.name}
                        onChange={(e) => {
                          const updated = [...headers];
                          updated[idx].name = e.target.value;
                          setHeaders(updated);
                        }}
                        placeholder="Header Name"
                        className="h-8 w-44 rounded-md border bg-background px-2.5 font-mono text-xs outline-none"
                      />
                      <input
                        type="text"
                        value={h.value}
                        onChange={(e) => {
                          const updated = [...headers];
                          updated[idx].value = e.target.value;
                          setHeaders(updated);
                        }}
                        placeholder="Value"
                        className="h-8 flex-1 rounded-md border bg-background px-2.5 font-mono text-xs outline-none"
                      />
                      <button
                        type="button"
                        onClick={() =>
                          setHeaders(headers.filter((_, i) => i !== idx))
                        }
                        className="text-muted-foreground hover:text-destructive p-1"
                      >
                        <Trash2 className="h-3.5 w-3.5" />
                      </button>
                    </div>
                  ))}
                  <button
                    type="button"
                    onClick={() =>
                      setHeaders([
                        ...headers,
                        { name: "", value: "", enabled: true },
                      ])
                    }
                    className="inline-flex items-center gap-1 text-xs text-primary hover:underline mt-1"
                  >
                    <Plus className="h-3 w-3" />
                    {t("curlAddHeader", "Add Header")}
                  </button>
                </div>
              ) : null}

              {/* Params Tab */}
              {requestTab === "params" ? (
                <div className="space-y-2">
                  {params.map((p, idx) => (
                    <div key={idx} className="flex items-center gap-2">
                      <input
                        type="checkbox"
                        checked={p.enabled}
                        onChange={(e) => {
                          const updated = [...params];
                          updated[idx].enabled = e.target.checked;
                          setParams(updated);
                        }}
                        className="h-4 w-4 rounded border-gray-300 text-primary"
                      />
                      <input
                        type="text"
                        value={p.name}
                        onChange={(e) => {
                          const updated = [...params];
                          updated[idx].name = e.target.value;
                          setParams(updated);
                        }}
                        placeholder="Key"
                        className="h-8 w-44 rounded-md border bg-background px-2.5 font-mono text-xs outline-none"
                      />
                      <input
                        type="text"
                        value={p.value}
                        onChange={(e) => {
                          const updated = [...params];
                          updated[idx].value = e.target.value;
                          setParams(updated);
                        }}
                        placeholder="Value"
                        className="h-8 flex-1 rounded-md border bg-background px-2.5 font-mono text-xs outline-none"
                      />
                      <button
                        type="button"
                        onClick={() =>
                          setParams(params.filter((_, i) => i !== idx))
                        }
                        className="text-muted-foreground hover:text-destructive p-1"
                      >
                        <Trash2 className="h-3.5 w-3.5" />
                      </button>
                    </div>
                  ))}
                  <button
                    type="button"
                    onClick={() =>
                      setParams([
                        ...params,
                        { name: "", value: "", enabled: true },
                      ])
                    }
                    className="inline-flex items-center gap-1 text-xs text-primary hover:underline mt-1"
                  >
                    <Plus className="h-3 w-3" />
                    {t("curlAddParam", "Add Parameter")}
                  </button>
                </div>
              ) : null}

              {/* Body Tab */}
              {requestTab === "body" ? (
                <div className="space-y-2">
                  <textarea
                    value={body}
                    onChange={(e) => setBody(e.target.value)}
                    placeholder='{"key": "value"}'
                    className="h-28 w-full resize-y rounded-md border bg-background p-2.5 font-mono text-xs leading-5 outline-none ring-offset-background focus-visible:ring-2 focus-visible:ring-ring"
                    spellCheck={false}
                  />
                </div>
              ) : null}
            </div>
          </div>
        </div>

        {/* 下半部分：Response 与 Code Export 双 Tab */}
        <div className="rounded-lg border bg-card p-4 space-y-3">
          <div className="flex items-center justify-between border-b pb-2">
            <div className="flex items-center gap-4 text-xs font-semibold">
              <button
                type="button"
                onClick={() => setBottomTab("response")}
                className={`pb-1 border-b-2 transition-colors ${
                  bottomTab === "response"
                    ? "border-primary text-foreground"
                    : "border-transparent text-muted-foreground hover:text-foreground"
                }`}
              >
                {t("curlTabResponse", "Response")}
              </button>
              <button
                type="button"
                onClick={() => setBottomTab("export")}
                className={`pb-1 border-b-2 transition-colors ${
                  bottomTab === "export"
                    ? "border-primary text-foreground"
                    : "border-transparent text-muted-foreground hover:text-foreground"
                }`}
              >
                {t("curlTabExport", "Export Code")}
              </button>
            </div>

            {bottomTab === "response" && response ? (
              <button
                type="button"
                onClick={() => void copyResponse(formattedResponseBody)}
                className="inline-flex items-center gap-1 text-xs text-muted-foreground hover:text-foreground"
              >
                {copiedResponse ? (
                  <Check className="h-3.5 w-3.5 text-emerald-600" />
                ) : (
                  <Copy className="h-3.5 w-3.5" />
                )}
                {t("curlCopyResponse", "Copy Body")}
              </button>
            ) : null}

            {bottomTab === "export" ? (
              <button
                type="button"
                onClick={() => void copyCode(generatedExportCode)}
                className="inline-flex items-center gap-1 text-xs text-muted-foreground hover:text-foreground"
              >
                {copiedCode ? (
                  <Check className="h-3.5 w-3.5 text-emerald-600" />
                ) : (
                  <Copy className="h-3.5 w-3.5" />
                )}
                {t("curlCopyCode", "Copy Code")}
              </button>
            ) : null}
          </div>

          {/* Response 面板 */}
          {bottomTab === "response" ? (
            <div>
              {loading ? (
                <div className="flex flex-col items-center justify-center py-12 text-muted-foreground text-xs gap-2">
                  <Loader2 className="h-5 w-5 animate-spin text-primary" />
                  <span>{t("curlSending", "Sending HTTP request...")}</span>
                </div>
              ) : response ? (
                <div className="space-y-3">
                  {/* 状态栏 */}
                  <div className="flex flex-wrap items-center gap-3 text-xs">
                    <ToolStatusBadge
                      tone={
                        response.status >= 200 && response.status < 300
                          ? "success"
                          : response.status >= 400
                          ? "error"
                          : "warning"
                      }
                      label={`${response.status} ${response.status_text || ""}`}
                    />
                    <div className="flex items-center gap-1 text-muted-foreground">
                      <Clock className="h-3.5 w-3.5" />
                      <span>{response.elapsed_ms} ms</span>
                    </div>
                    <div className="text-muted-foreground">
                      {(response.body.length / 1024).toFixed(1)} KB
                    </div>
                    {response.is_truncated ? (
                      <span className="text-[11px] text-amber-500">
                        ({t("curlTruncatedNotice", "Response truncated at 5MB")})
                      </span>
                    ) : null}
                  </div>

                  {/* 响应正文 */}
                  <pre className="max-h-80 overflow-auto rounded-md bg-muted/60 p-3 font-mono text-xs leading-5 text-foreground">
                    {formattedResponseBody || "(Empty Body)"}
                  </pre>
                </div>
              ) : (
                <div className="py-12 text-center text-xs text-muted-foreground">
                  {t(
                    "curlNoResponseYet",
                    "Click 'Send' to make the request and view the response.",
                  )}
                </div>
              )}
            </div>
          ) : null}

          {/* Code Export 面板 */}
          {bottomTab === "export" ? (
            <div className="space-y-3">
              <div className="flex items-center gap-2">
                <span className="text-xs text-muted-foreground">
                  {t("curlSelectLanguage", "Language")}:
                </span>
                <select
                  value={exportLang}
                  onChange={(e) =>
                    setExportLang(e.target.value as CodeExportLanguage)
                  }
                  className="h-8 rounded-md border bg-background px-2.5 text-xs outline-none"
                >
                  <option value="typescript-fetch">TypeScript (fetch)</option>
                  <option value="python-requests">Python (requests)</option>
                  <option value="go-nethttp">Go (net/http)</option>
                  <option value="rust-reqwest">Rust (reqwest)</option>
                  <option value="curl">cURL (Normalized)</option>
                </select>
              </div>

              <pre className="max-h-80 overflow-auto rounded-md bg-muted/60 p-3 font-mono text-xs leading-5 text-foreground">
                {generatedExportCode}
              </pre>
            </div>
          ) : null}
        </div>
      </div>
    </ToolShell>
  );
}
