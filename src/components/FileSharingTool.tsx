import { useCallback, useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { Copy, FilePlus2, RefreshCw, ShieldAlert, Square, Trash2 } from "lucide-react";
import { QRCodeSVG } from "qrcode.react";
import { useTranslation } from "react-i18next";
import {
  fileSharingNetworks,
  fileSharingStart,
  fileSharingStatus,
  fileSharingStop,
  subscribeFileSharingUpdates,
  type FileSharingNetwork,
  type FileSharingSnapshot,
} from "@/lib/fileSharing";
import { getMoreToolPresentation } from "@/lib/moreToolPresentation";

const EMPTY_SNAPSHOT: FileSharingSnapshot = {
  running: false,
  sessionId: null,
  address: null,
  port: null,
  shareUrl: null,
  startedAt: null,
  stoppedAt: null,
  files: [],
  transfers: [],
  summary: { activeTransfers: 0, completedTransfers: 0, failedTransfers: 0, cancelledTransfers: 0, bytesSent: 0, droppedTransferRecords: 0 },
  lastError: null,
};

function formatBytes(value: number) {
  if (value < 1024) return `${value} B`;
  if (value < 1024 * 1024) return `${(value / 1024).toFixed(1)} KB`;
  return `${(value / (1024 * 1024)).toFixed(1)} MB`;
}

function messageFor(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

export function FileSharingTool({ isVisible = true }: { isVisible?: boolean }) {
  const { t } = useTranslation();
  const { icon: ToolIcon, iconClassName } = getMoreToolPresentation("file-sharing");
  const isTauri = "__TAURI_INTERNALS__" in window;
  const [paths, setPaths] = useState<string[]>([]);
  const [networks, setNetworks] = useState<FileSharingNetwork[]>([]);
  const [networkId, setNetworkId] = useState("");
  const [snapshot, setSnapshot] = useState<FileSharingSnapshot>(EMPTY_SNAPSHOT);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const requestVersion = useRef(0);

  const refreshStatus = useCallback(async () => {
    if (!isTauri) return;
    const version = ++requestVersion.current;
    try {
      const next = await fileSharingStatus();
      if (version === requestVersion.current) setSnapshot(next);
    } catch (nextError) {
      if (version === requestVersion.current) setError(messageFor(nextError));
    }
  }, [isTauri]);

  const refreshNetworks = useCallback(async () => {
    if (!isTauri) return;
    try {
      const next = await fileSharingNetworks();
      setNetworks(next);
      setNetworkId((current) => next.some((network) => network.id === current) ? current : next[0]?.id || "");
    } catch (nextError) {
      setError(messageFor(nextError));
    }
  }, [isTauri]);

  useEffect(() => {
    if (!isTauri) return;
    void refreshNetworks();
    if (isVisible) void refreshStatus();
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void subscribeFileSharingUpdates(() => {
      if (!disposed) void refreshStatus();
    }).then((fn) => {
      if (disposed) fn(); else unlisten = fn;
    }).catch(() => {});
    return () => { disposed = true; unlisten?.(); };
  }, [isTauri, isVisible, refreshNetworks, refreshStatus]);

  const chooseFiles = async () => {
    if (!isTauri) return;
    const selected = await open({ multiple: true, directory: false });
    if (!selected) return;
    const next = (Array.isArray(selected) ? selected : [selected]).filter((path): path is string => typeof path === "string");
    setPaths((current) => [...new Set([...current, ...next])]);
  };

  const start = async () => {
    if (!networkId || paths.length === 0) return;
    requestVersion.current += 1;
    setLoading(true);
    setError(null);
    try {
      setSnapshot(await fileSharingStart({ networkId, paths }));
    } catch (nextError) {
      setError(messageFor(nextError));
    } finally {
      setLoading(false);
    }
  };

  const stop = async () => {
    if (snapshot.summary.activeTransfers > 0 && !window.confirm(t("fileSharingStopActiveConfirm", "Stopping now will interrupt active downloads. Continue?"))) return;
    requestVersion.current += 1;
    setLoading(true);
    try {
      setSnapshot(await fileSharingStop());
    } catch (nextError) {
      setError(messageFor(nextError));
    } finally {
      setLoading(false);
    }
  };

  const copyLink = async () => {
    if (!snapshot.shareUrl) return;
    try { await navigator.clipboard.writeText(snapshot.shareUrl); } catch { setError(t("fileSharingCopyFailed", "Could not copy the sharing link.")); }
  };

  if (!isTauri) {
    return <div className="max-w-2xl space-y-3"><h2 className="text-xl font-semibold">{t("fileSharing", "File Sharing")}</h2><p className="text-sm text-muted-foreground">{t("fileSharingDesktopRequired", "File sharing requires the OneSpace desktop app.")}</p></div>;
  }

  return (
    <div className="mx-auto flex h-full max-w-5xl flex-col gap-5 overflow-hidden">
      <div className="flex items-start gap-3">
        <div className={`rounded-lg p-2 ${iconClassName}`}>
          <ToolIcon className="h-5 w-5" />
        </div>
        <div>
          <h2 className="text-lg font-semibold">{t("fileSharing", "File Sharing")}</h2>
          <p className="text-sm text-muted-foreground">{t("fileSharingDesc", "Share selected files over a trusted local network.")}</p>
        </div>
      </div>
      {error ? <div role="alert" className="rounded-md border border-destructive/30 bg-destructive/10 p-3 text-sm text-destructive break-words">{error}</div> : null}
      {snapshot.running ? (
        <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto">
          <div className="flex items-start gap-3 rounded-md border border-amber-500/30 bg-amber-500/10 p-3 text-sm text-amber-800 dark:text-amber-200"><ShieldAlert className="mt-0.5 h-5 w-5 shrink-0" />{t("fileSharingWarning", "Use this HTTP link only on a trusted local network. Anyone with the link can download these files while sharing is active.")}</div>
          <div className="grid gap-4 md:grid-cols-[220px_minmax(0,1fr)]">
            {snapshot.shareUrl ? <div className="flex justify-center rounded-md border bg-white p-4"><QRCodeSVG value={snapshot.shareUrl} size={180} /></div> : null}
            <div className="min-w-0 space-y-3"><div className="rounded-md border p-3 font-mono text-sm break-all">{snapshot.shareUrl}</div><button type="button" onClick={copyLink} className="inline-flex h-9 items-center gap-2 rounded-md border px-3 text-sm hover:bg-muted"><Copy className="h-4 w-4" />{t("fileSharingCopyLink", "Copy link")}</button><p className="text-sm text-muted-foreground">{t("fileSharingStarted", "Started")}: {snapshot.startedAt ? new Date(snapshot.startedAt).toLocaleString() : "-"}</p></div>
          </div>
          <section><h3 className="mb-2 text-sm font-semibold">{t("fileSharingFiles", "Shared files")} ({snapshot.files.length})</h3><div className="max-h-48 overflow-y-auto rounded-md border">{snapshot.files.map((file) => <div key={file.id} className="flex items-center justify-between gap-4 border-b p-3 text-sm last:border-0"><span className="min-w-0 truncate" title={file.name}>{file.name}</span><span className="shrink-0 text-muted-foreground">{formatBytes(file.size)}</span></div>)}</div></section>
          <section><h3 className="mb-2 text-sm font-semibold">{t("fileSharingTransfers", "Transfers")}</h3><div className="max-h-48 overflow-y-auto rounded-md border">{snapshot.transfers.length ? snapshot.transfers.map((transfer) => <div key={transfer.id} className="grid grid-cols-[minmax(0,1fr)_auto] gap-4 border-b p-3 text-sm last:border-0"><span className="truncate">{transfer.fileName} · {transfer.clientAddress}</span><span>{t(`fileSharingState_${transfer.state}`, transfer.state)} · {formatBytes(transfer.bytesSent)}</span></div>) : <p className="p-3 text-sm text-muted-foreground">{t("fileSharingNoTransfers", "No transfers yet.")}</p>}</div></section>
          <div className="text-sm text-muted-foreground">{t("fileSharingSummary", "Completed {{completed}}, failed {{failed}}, sent {{bytes}}", { completed: snapshot.summary.completedTransfers, failed: snapshot.summary.failedTransfers + snapshot.summary.cancelledTransfers, bytes: formatBytes(snapshot.summary.bytesSent) })}</div>
          <button type="button" onClick={stop} disabled={loading} className="inline-flex h-10 w-fit items-center gap-2 rounded-md bg-destructive px-4 text-sm font-medium text-destructive-foreground disabled:opacity-50"><Square className="h-4 w-4" />{t("fileSharingStop", "Stop sharing")}</button>
        </div>
      ) : (
        <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto">
          {snapshot.sessionId ? <section className="space-y-3"><h3 className="text-sm font-semibold">{t("fileSharingEnded", "Sharing ended")}</h3><p className="text-sm text-muted-foreground">{t("fileSharingSummary", "Completed {{completed}}, failed {{failed}}, sent {{bytes}}", { completed: snapshot.summary.completedTransfers, failed: snapshot.summary.failedTransfers + snapshot.summary.cancelledTransfers, bytes: formatBytes(snapshot.summary.bytesSent) })}</p><div className="max-h-40 overflow-y-auto rounded-md border">{snapshot.files.map((file) => <div key={file.id} className="flex items-center justify-between gap-4 border-b p-3 text-sm last:border-0"><span className="min-w-0 truncate" title={file.name}>{file.name}</span><span className="shrink-0 text-muted-foreground">{formatBytes(file.size)}</span></div>)}</div><div className="max-h-40 overflow-y-auto rounded-md border">{snapshot.transfers.length ? snapshot.transfers.map((transfer) => <div key={transfer.id} className="grid grid-cols-[minmax(0,1fr)_auto] gap-4 border-b p-3 text-sm last:border-0"><span className="truncate">{transfer.fileName} · {transfer.clientAddress}</span><span>{t(`fileSharingState_${transfer.state}`, transfer.state)} · {formatBytes(transfer.bytesSent)}</span></div>) : <p className="p-3 text-sm text-muted-foreground">{t("fileSharingNoTransfers", "No transfers yet.")}</p>}</div></section> : null}
          <div className="flex flex-wrap gap-2"><button type="button" onClick={() => void chooseFiles()} className="inline-flex h-10 items-center gap-2 rounded-md border px-3 text-sm hover:bg-muted"><FilePlus2 className="h-4 w-4" />{t("fileSharingChooseFiles", "Choose files")}</button><button type="button" onClick={() => setPaths([])} disabled={paths.length === 0} className="inline-flex h-10 items-center gap-2 rounded-md border px-3 text-sm hover:bg-muted disabled:opacity-50"><Trash2 className="h-4 w-4" />{t("fileSharingClearFiles", "Clear")}</button></div>
          <div className="max-h-56 overflow-y-auto rounded-md border">{paths.length ? paths.map((path) => <div key={path} className="flex items-center justify-between gap-3 border-b p-3 text-sm last:border-0"><span className="min-w-0 truncate" title={path}>{path}</span><button type="button" aria-label={t("fileSharingRemoveFile", "Remove file")} onClick={() => setPaths((current) => current.filter((item) => item !== path))} className="p-1 text-muted-foreground hover:text-destructive"><Trash2 className="h-4 w-4" /></button></div>) : <p className="p-3 text-sm text-muted-foreground">{t("fileSharingNoFiles", "Choose one or more files to share.")}</p>}</div>
          <div className="flex flex-wrap items-end gap-2"><label className="min-w-64 flex-1 text-sm font-medium">{t("fileSharingNetwork", "Network address")}<select value={networkId} onChange={(event) => setNetworkId(event.target.value)} className="mt-1 flex h-10 w-full rounded-md border bg-background px-3 text-sm"><option value="">{t("fileSharingSelectNetwork", "Select a private network")}</option>{networks.map((network) => <option key={network.id} value={network.id}>{network.interfaceName} · {network.address}</option>)}</select></label><button type="button" onClick={() => void refreshNetworks()} className="inline-flex h-10 items-center gap-2 rounded-md border px-3 text-sm hover:bg-muted"><RefreshCw className="h-4 w-4" />{t("fileSharingRescan", "Rescan")}</button></div>
          {networks.length === 0 ? <p className="text-sm text-muted-foreground">{t("fileSharingNoNetworks", "No private IPv4 address is available.")}</p> : null}
          <button type="button" onClick={() => void start()} disabled={loading || !networkId || paths.length === 0} className="inline-flex h-10 w-fit items-center gap-2 rounded-md bg-primary px-4 text-sm font-medium text-primary-foreground disabled:opacity-50"><FilePlus2 className="h-4 w-4" />{loading ? t("fileSharingStarting", "Starting...") : t("fileSharingStart", "Start sharing")}</button>
        </div>
      )}
    </div>
  );
}
