import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { AlertTriangle, Check, ChevronLeft, ChevronRight, Copy, Download, RefreshCw, Trash2 } from "lucide-react";
import { save } from "@tauri-apps/plugin-dialog";
import { useConfirmDialog } from "@/components/ConfirmDialogProvider";
import { Switch } from "@/components/ui/switch";
import { errorToMessage } from "@/lib/messages";
import {
  aiRequestCaptureGenerateCurl,
  aiRequestCaptureClear,
  aiRequestCaptureExportHar,
  aiRequestCaptureGet,
  aiRequestCaptureGetConfig,
  aiRequestCaptureList,
  aiRequestCaptureSaveConfig,
  aiRequestCaptureStatus,
  subscribeAiRequestCaptureStatus,
  subscribeAiRequestCaptureUpdates,
  type AiRequestCaptureConfig,
  type AiRequestCaptureDetail,
  type AiRequestCaptureListItem,
  type AiRequestCaptureStatus,
  type CaptureListQuery,
  type CaptureState,
  type CapturedBody,
} from "@/lib/aiRequestCapture";

const PAGE_SIZE = 20;
const REFRESH_DEBOUNCE_MS = 250;
const DEFAULT_CONFIG: AiRequestCaptureConfig = { enabled: false, port: 17688, upstreamBaseUrl: "" };
const STATES: CaptureState[] = ["in_progress", "completed", "rejected", "upstream_error", "request_transfer_error", "response_transfer_error", "client_disconnected", "interrupted"];
const STATE_LABEL_FALLBACKS: Record<CaptureState, string> = {
  in_progress: "In progress",
  completed: "Completed",
  rejected: "Rejected",
  upstream_error: "Upstream error",
  request_transfer_error: "Request transfer error",
  response_transfer_error: "Response transfer error",
  client_disconnected: "Client disconnected",
  interrupted: "Interrupted",
};

type DetailTab = "overview" | "request" | "response";
type BodyTab = "headers" | "body";

function formatJsonBody(body: CapturedBody) {
  if (body.encoding || !body.data) return body.data;
  try {
    return JSON.stringify(JSON.parse(body.data), null, 2);
  } catch {
    return body.data;
  }
}

function compactQuery(query: CaptureListQuery): CaptureListQuery {
  return {
    ...query,
    search: query.search || undefined,
    method: query.method || undefined,
    provider: query.provider || undefined,
    model: query.model || undefined,
    states: query.states?.length ? query.states : undefined,
  };
}

function stateLabel(
  state: CaptureState,
  t: (key: string, fallback: string) => string,
) {
  return t(`aiRequestCaptureState_${state}`, STATE_LABEL_FALLBACKS[state]);
}

function statusTone(state: CaptureState) {
  if (state === "completed") return "bg-emerald-500/10 text-emerald-700";
  if (state === "in_progress") return "bg-amber-500/10 text-amber-700";
  return "bg-destructive/10 text-destructive";
}

function CopyButton({ label, value }: { label: string; value: string }) {
  const { t } = useTranslation();
  const confirmDialog = useConfirmDialog();
  const [copied, setCopied] = useState(false);

  const copy = async () => {
    if (value.startsWith("# WARNING:")) {
      const warning = value.split("\n", 1)[0].replace(/^# WARNING:\s*/, "");
      const confirmed = await confirmDialog(
        t("aiRequestCaptureIncompleteCurlConfirm", "This cURL contains real authentication headers, Cookies, and request body values. {{warning}} Copy anyway?", { warning }),
        { title: t("aiRequestCaptureIncompleteCurlTitle", "Incomplete cURL"), kind: "warning", okLabel: t("aiRequestCaptureCopyAnyway", "Copy anyway") },
      );
      if (!confirmed) return;
    }
    await navigator.clipboard.writeText(value);
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1600);
  };

  return (
    <button
      type="button"
      className="inline-flex h-8 w-8 shrink-0 items-center justify-center rounded-md border text-muted-foreground hover:bg-muted"
      aria-label={label}
      title={label}
      onClick={() => void copy()}
    >
      {copied ? <Check className="h-4 w-4 text-emerald-600" /> : <Copy className="h-4 w-4" />}
    </button>
  );
}

function ValueRow({ label, value, copyLabel }: { label: string; value: string; copyLabel: string }) {
  return (
    <div className="grid min-w-0 grid-cols-[112px_minmax(0,1fr)_32px] items-center gap-2">
      <span className="text-xs text-muted-foreground">{label}</span>
      <code className="min-w-0 truncate text-xs" title={value}>{value || "--"}</code>
      {value ? <CopyButton label={copyLabel} value={value} /> : <span />}
    </div>
  );
}

function HeaderList({ headers }: { headers: AiRequestCaptureDetail["requestHeaders"] }) {
  if (!headers.length) return <p className="text-sm text-muted-foreground">--</p>;
  return (
    <dl className="space-y-2 font-mono text-xs">
      {headers.map((header) => (
        <div key={`${header.name}:${header.values.join("\u0000")}`} className="grid min-w-0 gap-1 border-b pb-2 sm:grid-cols-[180px_minmax(0,1fr)]">
          <dt className="break-all text-muted-foreground">{header.name}</dt>
          <dd className="min-w-0 break-all whitespace-pre-wrap">{header.values.join(", ")}</dd>
        </div>
      ))}
    </dl>
  );
}

function BodyView({ body, t }: { body: CapturedBody; t: (key: string, fallback: string) => string }) {
  return (
    <div className="min-w-0 space-y-2">
      <div className="flex flex-wrap gap-x-3 gap-y-1 text-xs text-muted-foreground">
        <span>{body.encoding || t("aiRequestCaptureText", "Text")}</span>
        <span>{body.capturedBytes} / {body.totalBytes} bytes</span>
        {body.truncated ? <span className="text-amber-700">{t("aiRequestCaptureTruncated", "Truncated")}</span> : null}
      </div>
      <pre className="max-h-96 min-w-0 overflow-auto rounded-md border bg-muted/20 p-3 whitespace-pre-wrap break-all text-xs leading-5">{formatJsonBody(body) || "--"}</pre>
      <CopyButton label={t("aiRequestCaptureCopyOriginal", "Copy original")} value={body.data} />
    </div>
  );
}

export function AiRequestCaptureTool({ isVisible = true }: { isVisible?: boolean }) {
  const { t } = useTranslation();
  const confirmDialog = useConfirmDialog();
  const isTauri = "__TAURI_INTERNALS__" in window;
  const [config, setConfig] = useState<AiRequestCaptureConfig>(DEFAULT_CONFIG);
  const [status, setStatus] = useState<AiRequestCaptureStatus | null>(null);
  const [items, setItems] = useState<AiRequestCaptureListItem[]>([]);
  const [total, setTotal] = useState(0);
  const [search, setSearch] = useState("");
  const [method, setMethod] = useState("");
  const [states, setStates] = useState<CaptureState[]>([]);
  const [provider, setProvider] = useState("");
  const [model, setModel] = useState("");
  const [page, setPage] = useState(1);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [detail, setDetail] = useState<AiRequestCaptureDetail | null>(null);
  const [curl, setCurl] = useState("");
  const [detailTab, setDetailTab] = useState<DetailTab>("overview");
  const [bodyTab, setBodyTab] = useState<BodyTab>("headers");
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [actionPending, setActionPending] = useState(false);
  const [message, setMessage] = useState("");
  const [notice, setNotice] = useState("");
  const listRequestRef = useRef(0);
  const detailRequestRef = useRef(0);
  const refreshTimerRef = useRef<number | null>(null);

  const query = useMemo<CaptureListQuery>(() => compactQuery({ search, method, states, provider, model, page, pageSize: PAGE_SIZE }), [search, method, states, provider, model, page]);
  const localBaseUrl = `http://${status?.listenAddress || "127.0.0.1"}:${status?.port || config.port}`;
  const pageCount = Math.max(1, Math.ceil(total / PAGE_SIZE));

  const loadList = useCallback(async (nextQuery: CaptureListQuery) => {
    const request = ++listRequestRef.current;
    setLoading(true);
    try {
      const result = await aiRequestCaptureList(nextQuery);
      if (request !== listRequestRef.current) return;
      setItems(result.items);
      setTotal(result.total);
      if (result.page !== page) setPage(result.page);
    } catch (error) {
      if (request === listRequestRef.current) setMessage(errorToMessage(error));
    } finally {
      if (request === listRequestRef.current) setLoading(false);
    }
  }, [page]);

  const loadDetail = useCallback(async (id: string) => {
    const request = ++detailRequestRef.current;
    try {
      const [nextDetail, nextCurl] = await Promise.all([aiRequestCaptureGet(id), aiRequestCaptureGenerateCurl(id)]);
      if (request !== detailRequestRef.current) return;
      setDetail(nextDetail);
      setCurl(nextCurl.command);
    } catch (error) {
      if (request === detailRequestRef.current) setMessage(errorToMessage(error));
    }
  }, []);

  const loadWorkspace = useCallback(async () => {
    if (!isTauri || !isVisible) return false;
    try {
      const [nextConfig, nextStatus] = await Promise.all([aiRequestCaptureGetConfig(), aiRequestCaptureStatus()]);
      setConfig(nextConfig);
      setStatus(nextStatus);
      await loadList(query);
      if (selectedId) await loadDetail(selectedId);
      return true;
    } catch (error) {
      setMessage(errorToMessage(error));
      return false;
    }
  }, [isTauri, isVisible, loadDetail, loadList, query, selectedId]);

  useEffect(() => {
    if (!isTauri || !isVisible) return;
    const timer = window.setTimeout(() => void loadList(query), search ? REFRESH_DEBOUNCE_MS : 0);
    return () => window.clearTimeout(timer);
  }, [isTauri, isVisible, loadList, query, search]);

  useEffect(() => {
    if (!isTauri || !isVisible) return;
    void Promise.all([aiRequestCaptureGetConfig(), aiRequestCaptureStatus()])
      .then(([nextConfig, nextStatus]) => {
        setConfig(nextConfig);
        setStatus(nextStatus);
      })
      .catch((error) => setMessage(errorToMessage(error)));
  }, [isTauri, isVisible]);

  useEffect(() => {
    if (!isTauri || !isVisible) return;
    let disposed = false;
    const scheduleRefresh = () => {
      if (disposed) return;
      if (refreshTimerRef.current !== null) window.clearTimeout(refreshTimerRef.current);
      refreshTimerRef.current = window.setTimeout(() => {
        refreshTimerRef.current = null;
        void loadWorkspace();
      }, REFRESH_DEBOUNCE_MS);
    };
    let unlistenUpdate: (() => void | Promise<void>) | undefined;
    let unlistenStatus: (() => void | Promise<void>) | undefined;
    void subscribeAiRequestCaptureUpdates(scheduleRefresh).then((unlisten) => {
      if (disposed) void unlisten(); else unlistenUpdate = unlisten;
    });
    void subscribeAiRequestCaptureStatus((nextStatus) => {
      setStatus(nextStatus);
      scheduleRefresh();
    }).then((unlisten) => {
      if (disposed) void unlisten(); else unlistenStatus = unlisten;
    });
    return () => {
      disposed = true;
      if (refreshTimerRef.current !== null) window.clearTimeout(refreshTimerRef.current);
      if (unlistenUpdate) void unlistenUpdate();
      if (unlistenStatus) void unlistenStatus();
    };
  }, [isTauri, isVisible, loadWorkspace]);

  useEffect(() => {
    if (page > pageCount) setPage(pageCount);
  }, [page, pageCount]);

  const selectItem = (id: string) => {
    setSelectedId(id);
    setDetail(null);
    setCurl("");
    setDetailTab("overview");
    setBodyTab("headers");
    void loadDetail(id);
  };

  const saveConfig = async () => {
    setSaving(true);
    setMessage("");
    try {
      const result = await aiRequestCaptureSaveConfig({ ...config, port: Number(config.port) });
      setConfig(result.config);
      setStatus(result.status);
      if (result.validationErrors.length) setMessage(result.validationErrors.map((error) => error.message).join("; "));
    } catch (error) {
      setMessage(errorToMessage(error));
    } finally {
      setSaving(false);
    }
  };

  const refreshWorkspace = async () => {
    setMessage("");
    setNotice("");
    if (await loadWorkspace()) setNotice(t("aiRequestCaptureRefreshed", "Requests refreshed."));
  };

  const exportHar = async () => {
    setMessage("");
    setNotice("");
    const outputPath = await save({
      defaultPath: "ai-request-captures.har",
      filters: [{ name: "HAR", extensions: ["har"] }],
    });
    if (!outputPath) return;
    const confirmed = await confirmDialog(
      t("aiRequestCaptureExportConfirm", "This HAR export and generated cURL commands contain plaintext authentication headers, Cookies, and request and response bodies with real credentials. Continue?"),
      { title: t("aiRequestCaptureExportConfirmTitle", "Export sensitive HAR"), kind: "warning", okLabel: t("aiRequestCaptureExportConfirmAction", "Export") },
    );
    if (!confirmed) return;

    const exportQuery = { ...query, page: 1, pageSize: PAGE_SIZE };
    setActionPending(true);
    try {
      const result = await aiRequestCaptureExportHar({ query: exportQuery, outputPath });
      setNotice(t("aiRequestCaptureExported", "Exported {{count}} requests.", { count: result.exported }));
      setPage(1);
      await loadList(exportQuery);
    } catch (error) {
      setMessage(errorToMessage(error));
    } finally {
      setActionPending(false);
    }
  };

  const clearHistory = async () => {
    setMessage("");
    setNotice("");
    const confirmed = await confirmDialog(
      t("aiRequestCaptureClearConfirm", "This permanently clears all non-in-progress request history and cannot be undone. In-progress requests may reappear after they complete."),
      { title: t("aiRequestCaptureClearConfirmTitle", "Clear request history"), kind: "warning", okLabel: t("aiRequestCaptureClearConfirmAction", "Clear") },
    );
    if (!confirmed) return;

    const firstPageQuery = { ...query, page: 1 };
    setActionPending(true);
    try {
      const result = await aiRequestCaptureClear();
      setSelectedId(null);
      setDetail(null);
      setCurl("");
      setPage(1);
      setNotice(t("aiRequestCaptureCleared", "Cleared {{count}} requests.", { count: result.cleared }));
      await loadList(firstPageQuery);
    } catch (error) {
      setMessage(errorToMessage(error));
    } finally {
      setActionPending(false);
    }
  };

  const changeStates = (state: CaptureState) => {
    setPage(1);
    setStates((current) => current.includes(state) ? current.filter((value) => value !== state) : [...current, state]);
  };

  return (
    <div className="h-full min-h-0 overflow-hidden p-4">
      <div className="flex h-full min-h-0 flex-col gap-4">
        <header className="shrink-0 space-y-3">
          <div className="flex flex-wrap items-start justify-between gap-3">
            <div>
              <h1 className="text-xl font-semibold">{t("aiRequestCapture", "AI Request Capture")}</h1>
              <p className="text-sm text-muted-foreground">{t("aiRequestCaptureDesc", "Inspect local proxy traffic and AI request metadata.")}</p>
            </div>
            <div className="flex items-center gap-2">
              <button type="button" onClick={() => void refreshWorkspace()} disabled={loading || actionPending} className="inline-flex h-9 w-9 items-center justify-center rounded-md border hover:bg-muted disabled:opacity-50" aria-label={t("refresh", "Refresh")} title={t("refresh", "Refresh")}>
                <RefreshCw className={`h-4 w-4 ${loading ? "animate-spin" : ""}`} />
              </button>
              <button type="button" onClick={() => void exportHar()} disabled={actionPending} className="inline-flex h-9 items-center gap-2 rounded-md border px-3 text-sm hover:bg-muted disabled:opacity-50">
                <Download className="h-4 w-4" />{t("aiRequestCaptureExportHar", "Export HAR")}
              </button>
              <button type="button" onClick={() => void clearHistory()} disabled={actionPending} className="inline-flex h-9 items-center gap-2 rounded-md border border-destructive/40 px-3 text-sm text-destructive hover:bg-destructive/5 disabled:opacity-50">
                <Trash2 className="h-4 w-4" />{t("aiRequestCaptureClearHistory", "Clear history")}
              </button>
            </div>
          </div>
          <div role="alert" className="flex items-start gap-2 rounded-md border border-amber-500/40 bg-amber-500/10 px-3 py-2 text-sm text-amber-900">
            <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" />
            {t("aiRequestCapturePlaintextWarning", "Captured headers and bodies are stored and displayed as plaintext, including sensitive credentials.")}
          </div>
          <section className="grid gap-3 rounded-md border bg-card p-3 lg:grid-cols-[auto_110px_minmax(220px,1fr)_auto] lg:items-end">
            <label className="flex items-center gap-2 text-sm font-medium"><Switch checked={config.enabled} onCheckedChange={(enabled) => setConfig((current) => ({ ...current, enabled }))} />{t("aiRequestCaptureEnabled", "Enabled")}</label>
            <label className="grid gap-1 text-xs text-muted-foreground">{t("aiRequestCapturePort", "Port")}<input aria-label={t("aiRequestCapturePort", "Port")} type="number" min="1" max="65535" value={config.port} onChange={(event) => setConfig((current) => ({ ...current, port: Number(event.target.value) }))} className="h-9 rounded-md border bg-background px-2 text-sm text-foreground" /></label>
            <label className="grid min-w-0 gap-1 text-xs text-muted-foreground">{t("aiRequestCaptureUpstreamBaseUrl", "Upstream Base URL")}<input aria-label={t("aiRequestCaptureUpstreamBaseUrl", "Upstream Base URL")} value={config.upstreamBaseUrl} onChange={(event) => setConfig((current) => ({ ...current, upstreamBaseUrl: event.target.value }))} className="h-9 min-w-0 rounded-md border bg-background px-2 text-sm text-foreground" /></label>
            <button type="button" onClick={() => void saveConfig()} disabled={saving} className="h-9 rounded-md bg-primary px-3 text-sm font-medium text-primary-foreground disabled:opacity-60">{t("aiRequestCaptureSaveApply", "Save and apply")}</button>
          </section>
          <section className="grid gap-2 rounded-md border bg-card p-3 text-xs sm:grid-cols-3">
            <ValueRow label={t("aiRequestCaptureStatus", "Status")} value={status?.running ? t("aiRequestCaptureStatusRunning", "Running") : t("aiRequestCaptureStatusStopped", "Stopped")} copyLabel={t("aiRequestCaptureCopyStatus", "Copy status")} />
            <ValueRow label={t("aiRequestCaptureLocalBaseUrl", "Local Base URL")} value={localBaseUrl} copyLabel={t("aiRequestCaptureCopyLocalBaseUrl", "Copy local Base URL")} />
            <ValueRow label={t("aiRequestCaptureUpstreamBaseUrl", "Upstream Base URL")} value={config.upstreamBaseUrl} copyLabel={t("aiRequestCaptureCopyUpstreamBaseUrl", "Copy upstream Base URL")} />
            {status?.lastError ? <p className="min-w-0 break-all text-destructive sm:col-span-3">{t("aiRequestCaptureLastError", "Last error")}: {status.lastError}</p> : null}
          </section>
          {message ? <p role="alert" className="rounded-md border border-destructive/30 bg-destructive/5 px-3 py-2 text-sm text-destructive">{message}</p> : null}
          {notice ? <p role="status" className="rounded-md border border-emerald-500/30 bg-emerald-500/5 px-3 py-2 text-sm text-emerald-700">{notice}</p> : null}
        </header>

        <main className="grid min-h-0 flex-1 gap-4 xl:grid-cols-[minmax(300px,0.8fr)_minmax(0,1.2fr)]">
          <section className="flex min-h-0 flex-col rounded-md border bg-card" aria-label={t("aiRequestCaptureRequests", "Requests")}>
            <div className="space-y-2 border-b p-3">
              <input aria-label={t("aiRequestCaptureSearch", "Search requests")} value={search} onChange={(event) => { setSearch(event.target.value); setPage(1); }} placeholder={t("aiRequestCaptureSearch", "Search requests")} className="h-9 w-full rounded-md border bg-background px-2 text-sm" />
              <div className="grid grid-cols-3 gap-2">
                <select aria-label={t("aiRequestCaptureMethod", "Method")} value={method} onChange={(event) => { setMethod(event.target.value); setPage(1); }} className="h-8 min-w-0 rounded-md border bg-background px-1 text-xs"><option value="">{t("all", "All")}</option><option>GET</option><option>POST</option><option>PUT</option><option>PATCH</option><option>DELETE</option></select>
                <input aria-label={t("provider", "Provider")} value={provider} onChange={(event) => { setProvider(event.target.value); setPage(1); }} placeholder={t("provider", "Provider")} className="h-8 min-w-0 rounded-md border bg-background px-2 text-xs" />
                <input aria-label={t("model", "Model")} value={model} onChange={(event) => { setModel(event.target.value); setPage(1); }} placeholder={t("model", "Model")} className="h-8 min-w-0 rounded-md border bg-background px-2 text-xs" />
              </div>
              <div className="flex max-h-16 flex-wrap gap-x-3 gap-y-1 overflow-y-auto text-xs">
                {STATES.map((state) => <label key={state} className="inline-flex items-center gap-1"><input type="checkbox" checked={states.includes(state)} onChange={() => changeStates(state)} />{stateLabel(state, t)}</label>)}
              </div>
            </div>
            <div className="min-h-0 flex-1 overflow-y-auto p-2">
              {items.map((item) => <button key={item.id} type="button" onClick={() => selectItem(item.id)} className={`mb-2 w-full rounded-md border p-3 text-left hover:bg-muted ${selectedId === item.id ? "border-primary bg-muted" : ""}`} aria-label={`${item.method} ${item.requestPathAndQuery} ${item.model || ""}`}>
                <div className="flex items-start justify-between gap-2"><span className="min-w-0 truncate font-mono text-xs">{item.method} {item.requestPathAndQuery}</span><span className={`shrink-0 rounded px-1.5 py-0.5 text-[10px] ${statusTone(item.state)}`}>{item.responseStatus || stateLabel(item.state, t)}</span></div>
                <div className="mt-2 flex min-w-0 flex-wrap gap-x-2 gap-y-1 text-xs text-muted-foreground"><span>{item.provider || "--"}</span><span className="truncate">{item.model || "--"}</span><span>{item.durationMs === null ? "--" : `${item.durationMs}ms`}</span></div>
              </button>)}
              {!items.length && !loading ? <p className="p-3 text-sm text-muted-foreground">{t("aiRequestCaptureNoRequests", "No captured requests match these filters.")}</p> : null}
            </div>
            <footer className="flex items-center justify-between border-t p-2 text-xs text-muted-foreground"><span>{total ? `${(page - 1) * PAGE_SIZE + 1}-${Math.min(total, page * PAGE_SIZE)} / ${total}` : "0 / 0"}</span><div className="flex items-center gap-1"><button type="button" aria-label={t("aiRequestCapturePreviousPage", "Previous page")} onClick={() => setPage((current) => Math.max(1, current - 1))} disabled={page === 1} className="h-7 w-7 rounded border disabled:opacity-40"><ChevronLeft className="mx-auto h-4 w-4" /></button><span>{page} / {pageCount}</span><button type="button" aria-label={t("aiRequestCaptureNextPage", "Next page")} onClick={() => setPage((current) => Math.min(pageCount, current + 1))} disabled={page === pageCount} className="h-7 w-7 rounded border disabled:opacity-40"><ChevronRight className="mx-auto h-4 w-4" /></button></div></footer>
          </section>

          <section className="flex min-h-0 flex-col rounded-md border bg-card">
            {!detail ? <div className="flex h-full items-center justify-center p-6 text-sm text-muted-foreground">{t("aiRequestCaptureSelectRequest", "Select a request to inspect its plaintext details.")}</div> : <>
              <div className="min-w-0 border-b p-3"><div className="flex min-w-0 items-start justify-between gap-2"><div className="min-w-0"><p className="truncate font-mono text-sm">{detail.method} {detail.requestPathAndQuery}</p><p className="truncate text-xs text-muted-foreground">{detail.upstreamUrl}</p></div><span className={`shrink-0 rounded px-2 py-1 text-xs ${statusTone(detail.state)}`}>{detail.responseStatus || stateLabel(detail.state, t)}</span></div><div role="tablist" className="mt-3 flex gap-1"><button role="tab" aria-selected={detailTab === "overview"} onClick={() => setDetailTab("overview")} className={`rounded-md px-3 py-1.5 text-sm ${detailTab === "overview" ? "bg-primary text-primary-foreground" : "hover:bg-muted"}`}>{t("aiRequestCaptureOverview", "Overview")}</button><button role="tab" aria-selected={detailTab === "request"} onClick={() => setDetailTab("request")} className={`rounded-md px-3 py-1.5 text-sm ${detailTab === "request" ? "bg-primary text-primary-foreground" : "hover:bg-muted"}`}>{t("aiRequestCaptureRequest", "Request")}</button><button role="tab" aria-selected={detailTab === "response"} onClick={() => setDetailTab("response")} className={`rounded-md px-3 py-1.5 text-sm ${detailTab === "response" ? "bg-primary text-primary-foreground" : "hover:bg-muted"}`}>{t("aiRequestCaptureResponse", "Response")}</button></div></div>
              <div className="min-h-0 flex-1 overflow-auto p-3">
                {detailTab === "overview" ? <div className="grid gap-3 text-sm sm:grid-cols-2"><ValueRow label={t("provider", "Provider")} value={detail.provider || ""} copyLabel={t("aiRequestCaptureCopyProvider", "Copy provider")} /><ValueRow label={t("model", "Model")} value={detail.model || ""} copyLabel={t("aiRequestCaptureCopyModel", "Copy model")} /><ValueRow label={t("aiRequestCaptureInputTokens", "Input tokens")} value={String(detail.inputTokens ?? "--")} copyLabel={t("aiRequestCaptureCopyInputTokens", "Copy input tokens")} /><ValueRow label={t("aiRequestCaptureOutputTokens", "Output tokens")} value={String(detail.outputTokens ?? "--")} copyLabel={t("aiRequestCaptureCopyOutputTokens", "Copy output tokens")} /><ValueRow label={t("tokens", "Tokens")} value={String(detail.totalTokens ?? "--")} copyLabel={t("aiRequestCaptureCopyTokens", "Copy tokens")} /><ValueRow label={t("aiRequestCaptureDuration", "Duration")} value={detail.durationMs === null ? "--" : `${detail.durationMs}ms`} copyLabel={t("aiRequestCaptureCopyDuration", "Copy duration")} />{detail.error ? <p className="break-all text-destructive sm:col-span-2">{t("aiRequestCaptureTransferError", "Transfer error")}: {detail.error}</p> : null}<div className="space-y-2 border-t pt-3 sm:col-span-2"><div className="flex items-center justify-between"><h2 className="text-sm font-medium">cURL</h2><CopyButton label={t("aiRequestCaptureCopyCurl", "Copy cURL")} value={curl} /></div><pre className="max-h-48 overflow-auto rounded-md border bg-muted/20 p-3 whitespace-pre-wrap break-all text-xs">{curl || "--"}</pre></div></div> : <><div role="tablist" className="mb-3 flex gap-1"><button role="tab" aria-selected={bodyTab === "headers"} onClick={() => setBodyTab("headers")} className={`rounded-md px-3 py-1.5 text-sm ${bodyTab === "headers" ? "bg-muted font-medium" : ""}`}>{t("aiRequestCaptureHeaders", "Headers")}</button><button role="tab" aria-selected={bodyTab === "body"} onClick={() => setBodyTab("body")} className={`rounded-md px-3 py-1.5 text-sm ${bodyTab === "body" ? "bg-muted font-medium" : ""}`}>{t("aiRequestCaptureBody", "Body")}</button></div>{bodyTab === "headers" ? <HeaderList headers={detailTab === "request" ? detail.requestHeaders : detail.responseHeaders} /> : <BodyView body={detailTab === "request" ? detail.requestBody : detail.responseBody} t={(key, fallback) => t(key, fallback)} />}</>}
              </div>
            </>}
          </section>
        </main>
      </div>
    </div>
  );
}
