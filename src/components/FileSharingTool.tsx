import { useCallback, useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import {
  Check,
  Copy,
  ExternalLink,
  FilePlus2,
  Lock,
  RefreshCw,
  ShieldAlert,
  Square,
  Trash2,
} from "lucide-react";
import { QRCodeSVG } from "qrcode.react";
import { useTranslation } from "react-i18next";
import { ToolErrorBanner } from "@/components/toolbox/ToolErrorBanner";
import { useToast } from "./ToastProvider";
import { CustomSelect } from "@/components/common/CustomSelect";
import { EnvironmentBanner } from "@/components/PortTunnelTool/EnvironmentBanner";
import { PasswordField } from "@/components/fileSharing/PasswordField";
import { PublicAccessCard } from "@/components/fileSharing/PublicAccessCard";
import { Switch } from "@/components/ui/switch";
import {
  FILE_SHARING_PUBLIC_UPDATED_EVENT,
  fileSharingNetworks,
  fileSharingPublicStart,
  fileSharingPublicStop,
  fileSharingStart,
  fileSharingStatus,
  fileSharingStop,
  getFileSharingPublicUrl,
  type FileSharingNetwork,
  type FileSharingStartInput,
  type FileSharingSnapshot,
} from "@/lib/fileSharing";
import { publishRuntimeStatus } from "@/lib/runtimeStatus";
import { tunnelCheckEnvironment, type EnvironmentStatus } from "@/lib/portTunnel";
import { getToolboxTool } from "@/toolbox/registry";
import { useCopyToClipboard } from "@/toolbox/useCopyToClipboard";
import { useTauriEvent } from "@/toolbox/useTauriEvent";

const EMPTY_SNAPSHOT: FileSharingSnapshot = {
  running: false,
  sessionId: null,
  shareUrl: null,
  startedAt: null,
  files: [],
  transfers: [],
  summary: { activeTransfers: 0, completedTransfers: 0, failedTransfers: 0, cancelledTransfers: 0, bytesSent: 0, droppedTransferRecords: 0 },
};

function formatBytes(value: number) {
  if (value < 1024) return `${value} B`;
  if (value < 1024 * 1024) return `${(value / 1024).toFixed(1)} KB`;
  return `${(value / (1024 * 1024)).toFixed(1)} MB`;
}

function messageFor(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

function isPasswordInvalid(value: string) {
  if (value.length === 0) return false;
  return value.length > 128 || !/\S/.test(value);
}

function isEnvironmentStatus(value: unknown): value is EnvironmentStatus {
  return (
    typeof value === "object" &&
    value !== null &&
    typeof (value as { installed?: unknown }).installed === "boolean"
  );
}

export function FileSharingTool({ isVisible = true }: { isVisible?: boolean }) {
  const { t } = useTranslation();
  const { pushToast } = useToast();
  const { icon: ToolIcon, iconClassName } = getToolboxTool("file-sharing")!;
  const { copied, copy: copyToClipboard } = useCopyToClipboard({
    onError: () =>
      pushToast({
        title: t("fileSharingCopyFailed", "Could not copy the sharing link."),
        kind: "error",
      }),
  });
  const isTauri = "__TAURI_INTERNALS__" in window;
  const [paths, setPaths] = useState<string[]>([]);
  const [networks, setNetworks] = useState<FileSharingNetwork[]>([]);
  const [networkId, setNetworkId] = useState("");
  const [snapshot, setSnapshot] = useState<FileSharingSnapshot>(EMPTY_SNAPSHOT);
  const [publicOn, setPublicOn] = useState(false);
  const [password, setPassword] = useState("");
  const [environment, setEnvironment] = useState<EnvironmentStatus | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const requestVersion = useRef(0);
  const wasRunning = useRef(false);

  const publicState = snapshot.publicState ?? "disabled";
  const publicActive = publicState === "starting" || publicState === "running";
  const wantsCloudflare = publicOn || (snapshot.running && publicState !== "disabled");
  const dependencyMissing = publicOn && environment?.installed === false;
  const passwordInvalid = isPasswordInvalid(password);
  const qrUrl = getFileSharingPublicUrl(snapshot);

  const refreshStatus = useCallback(async () => {
    if (!isTauri) return;
    const version = ++requestVersion.current;
    try {
      const next = await fileSharingStatus();
      if (version !== requestVersion.current) return;
      setSnapshot(next);
      publishRuntimeStatus("file-sharing", next);
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
  }, [isTauri, isVisible, refreshNetworks, refreshStatus]);

  useEffect(() => {
    if (!isTauri || !wantsCloudflare) {
      setEnvironment(null);
      return;
    }
    let cancelled = false;
    void tunnelCheckEnvironment("cloudflare")
      .then((next) => {
        if (!cancelled) setEnvironment(isEnvironmentStatus(next) ? next : null);
      })
      .catch(() => {
        if (!cancelled) setEnvironment(null);
      });
    return () => {
      cancelled = true;
    };
  }, [isTauri, wantsCloudflare]);

  useEffect(() => {
    if (wasRunning.current && !snapshot.running) {
      setPublicOn(false);
      setPassword("");
      setEnvironment(null);
    }
    wasRunning.current = snapshot.running;
  }, [snapshot.running]);

  useTauriEvent(
    "file-sharing-updated",
    () => {
      void refreshStatus();
    },
    isVisible,
    { respectVisibility: true },
  );

  useTauriEvent(
    FILE_SHARING_PUBLIC_UPDATED_EVENT,
    (payload) => {
      const tunnelId =
        typeof payload === "object" && payload !== null && "id" in payload
          ? (payload as { id?: unknown }).id
          : undefined;
      if (typeof tunnelId !== "string" || tunnelId !== snapshot.publicTunnelId) return;
      void refreshStatus();
    },
    isVisible,
    { respectVisibility: true },
  );

  const chooseFiles = async () => {
    if (!isTauri) return;
    const selected = await open({ multiple: true, directory: false });
    if (!selected) return;
    const next = (Array.isArray(selected) ? selected : [selected]).filter((path): path is string => typeof path === "string");
    setPaths((current) => [...new Set([...current, ...next])]);
  };

  const start = async () => {
    if (!networkId || paths.length === 0 || passwordInvalid) return;
    if (dependencyMissing) return;
    requestVersion.current += 1;
    setLoading(true);
    setError(null);
    try {
      const input: FileSharingStartInput = { networkId, paths };
      if (publicOn) input.publicAccess = true;
      if (password.length > 0) input.password = password;
      const next = await fileSharingStart(input);
      setSnapshot(next);
      setPassword("");
      publishRuntimeStatus("file-sharing", next);
    } catch (nextError) {
      setError(messageFor(nextError));
    } finally {
      setLoading(false);
    }
  };

  const stop = async () => {
    requestVersion.current += 1;
    setLoading(true);
    try {
      const next = await fileSharingStop();
      setSnapshot(next);
      setPublicOn(false);
      setPassword("");
      setEnvironment(null);
      publishRuntimeStatus("file-sharing", next);
    } catch (nextError) {
      setError(messageFor(nextError));
    } finally {
      setLoading(false);
    }
  };

  const enablePublic = async () => {
    requestVersion.current += 1;
    setLoading(true);
    setError(null);
    try {
      const next = await fileSharingPublicStart();
      setSnapshot(next);
      publishRuntimeStatus("file-sharing", next);
    } catch (nextError) {
      setError(messageFor(nextError));
    } finally {
      setLoading(false);
    }
  };

  const disablePublic = async () => {
    requestVersion.current += 1;
    setLoading(true);
    setError(null);
    try {
      const next = await fileSharingPublicStop();
      setSnapshot(next);
      publishRuntimeStatus("file-sharing", next);
    } catch (nextError) {
      setError(messageFor(nextError));
    } finally {
      setLoading(false);
    }
  };

  const copyPublicLink = async () => {
    if (!snapshot.publicShareUrl) return;
    await copyToClipboard(snapshot.publicShareUrl);
  };

  const copyLink = async () => {
    if (!snapshot.shareUrl) return;
    await copyToClipboard(snapshot.shareUrl);
  };

  const startForm = (
    <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto">
      {!snapshot.running && snapshot.sessionId ? (
        <section className="space-y-3">
          <h3 className="text-sm font-semibold">{t("fileSharingEnded", "Sharing ended")}</h3>
          <p className="text-sm text-muted-foreground">{t("fileSharingSummary", "Completed {{completed}}, failed {{failed}}, sent {{bytes}}", { completed: snapshot.summary.completedTransfers, failed: snapshot.summary.failedTransfers + snapshot.summary.cancelledTransfers, bytes: formatBytes(snapshot.summary.bytesSent) })}</p>
          <div className="max-h-40 overflow-y-auto rounded-md border">{snapshot.files.map((file) => <div key={file.id} className="flex items-center justify-between gap-4 border-b p-3 text-sm last:border-0"><span className="min-w-0 truncate" title={file.name}>{file.name}</span><span className="shrink-0 text-muted-foreground">{formatBytes(file.size)}</span></div>)}</div>
          <div className="max-h-40 overflow-y-auto rounded-md border">{snapshot.transfers.length ? snapshot.transfers.map((transfer) => <div key={transfer.id} className="grid grid-cols-[minmax(0,1fr)_auto] gap-4 border-b p-3 text-sm last:border-0"><span className="truncate">{transfer.fileName} · {transfer.clientAddress}</span><span>{t(`fileSharingState_${transfer.state}`, transfer.state)} · {formatBytes(transfer.bytesSent)}</span></div>) : <p className="p-3 text-sm text-muted-foreground">{t("fileSharingNoTransfers", "No transfers yet.")}</p>}</div>
        </section>
      ) : null}
      <div className="flex flex-wrap gap-2">
        <button type="button" onClick={() => void chooseFiles()} className="inline-flex h-10 items-center gap-2 rounded-md border px-3 text-sm hover:bg-muted"><FilePlus2 className="h-4 w-4" />{t("fileSharingChooseFiles", "Choose files")}</button>
        <button type="button" onClick={() => setPaths([])} disabled={paths.length === 0} className="inline-flex h-10 items-center gap-2 rounded-md border px-3 text-sm hover:bg-muted disabled:opacity-50"><Trash2 className="h-4 w-4" />{t("fileSharingClearFiles", "Clear")}</button>
      </div>
      <div className="max-h-56 overflow-y-auto rounded-md border">{paths.length ? paths.map((path) => <div key={path} className="flex items-center justify-between gap-3 border-b p-3 text-sm last:border-0"><span className="min-w-0 truncate" title={path}>{path}</span><button type="button" aria-label={t("fileSharingRemoveFile", "Remove file")} onClick={() => setPaths((current) => current.filter((item) => item !== path))} className="p-1 text-muted-foreground hover:text-destructive"><Trash2 className="h-4 w-4" /></button></div>) : <p className="p-3 text-sm text-muted-foreground">{t("fileSharingNoFiles", "Choose one or more files to share.")}</p>}</div>
      <div className="flex flex-wrap items-end gap-2"><div className="min-w-64 flex-1 space-y-1"><span className="text-sm font-medium">{t("fileSharingNetwork", "Network address")}</span><CustomSelect value={networkId} onChange={(val) => setNetworkId(val)} ariaLabel={t("fileSharingNetwork", "Network address")} placeholder={t("fileSharingSelectNetwork", "Select a private network")} options={[{ value: "", label: t("fileSharingSelectNetwork", "Select a private network") }, ...networks.map((network) => ({ value: network.id, label: `${network.interfaceName} · ${network.address}` }))]} /></div><button type="button" onClick={() => void refreshNetworks()} className="inline-flex h-10 items-center gap-2 rounded-md border px-3 text-sm hover:bg-muted"><RefreshCw className="h-4 w-4" />{t("fileSharingRescan", "Rescan")}</button></div>
      {networks.length === 0 ? <p className="text-sm text-muted-foreground">{t("fileSharingNoNetworks", "No private IPv4 address is available.")}</p> : null}
      <section className="space-y-3 rounded-md border p-3">
        <div className="flex items-start justify-between gap-3">
          <div className="space-y-1">
            <span className="text-sm font-medium">{t("fileSharingPublicTitle", "Public access")}</span>
            <p className="text-xs text-muted-foreground">{t("fileSharingPublicTemporary", "The Quick Tunnel link is temporary and changes when public access restarts.")}</p>
          </div>
          <Switch
            data-testid="file-sharing-public-toggle"
            checked={publicOn}
            onCheckedChange={setPublicOn}
            aria-label={t("fileSharingPublicTitle", "Public access")}
          />
        </div>
        {dependencyMissing ? (
          <p className="text-xs text-destructive">{t("fileSharingPublicMissingDependency", "cloudflared is required for public sharing. Install it and try again.")}</p>
        ) : null}
        <PasswordField
          value={password}
          invalid={passwordInvalid}
          onChange={setPassword}
        />
      </section>
      <button type="button" onClick={() => void start()} disabled={loading || !networkId || paths.length === 0 || passwordInvalid || dependencyMissing || snapshot.running} className="inline-flex h-10 w-fit items-center gap-2 rounded-md bg-primary px-4 text-sm font-medium text-primary-foreground disabled:opacity-50"><FilePlus2 className="h-4 w-4" />{loading ? t("fileSharingStarting", "Starting...") : t("fileSharingStart", "Start sharing")}</button>
    </div>
  );

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
      <ToolErrorBanner message={error} />
      {snapshot.running ? (
        <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto">
          {publicActive || publicState === "error" ? <EnvironmentBanner status={environment} /> : null}
          <div className="flex items-start gap-3 rounded-md border border-amber-500/30 bg-amber-500/10 p-3 text-sm text-amber-800 dark:text-amber-200"><ShieldAlert className="mt-0.5 h-5 w-5 shrink-0" />{t("fileSharingWarning", "Use this HTTP link only on a trusted local network. Anyone with the link can download these files while sharing is active.")}</div>
          {publicActive ? (
            <div className="flex items-start gap-3 rounded-md border border-amber-500/30 bg-amber-500/10 p-3 text-sm text-amber-800 dark:text-amber-200"><ShieldAlert className="mt-0.5 h-5 w-5 shrink-0" />{t("fileSharingPublicExposure", "When public access is on, anyone on the internet with the link can download the shared files.")} {t("fileSharingPublicTemporary", "The Quick Tunnel link is temporary and changes when public access restarts.")}</div>
          ) : null}
          <div className="grid gap-4 md:grid-cols-[220px_minmax(0,1fr)]">
            {qrUrl ? <div className="flex justify-center rounded-md border bg-white p-4"><QRCodeSVG value={qrUrl} size={180} /></div> : null}
            <div className="min-w-0 space-y-3">
              <div className="space-y-2 rounded-md border p-3">
                <div className="flex flex-wrap items-center justify-between gap-2">
                  <span className="text-sm font-semibold">{t("fileSharingLanLink", "LAN link")}</span>
                  {snapshot.passwordProtected ? (
                    <span className="inline-flex items-center gap-1 rounded-full border px-2 py-0.5 text-xs text-muted-foreground"><Lock className="h-3 w-3" />{t("fileSharingPasswordProtected", "Password protected")}</span>
                  ) : null}
                </div>
                <div data-testid="file-sharing-lan-url" className="font-mono text-sm break-all">{snapshot.shareUrl}</div>
                <div className="flex flex-wrap gap-2">
                  <button type="button" onClick={() => void copyLink()} className="inline-flex h-9 items-center gap-2 rounded-md border px-3 text-sm hover:bg-muted">{copied ? <Check className="h-4 w-4 text-emerald-600" /> : <Copy className="h-4 w-4" />}{t("fileSharingCopyLink", "Copy link")}</button>
                  <button type="button" onClick={() => { if (snapshot.shareUrl) window.open(snapshot.shareUrl, "_blank", "noopener,noreferrer"); }} className="inline-flex h-9 items-center gap-2 rounded-md border px-3 text-sm hover:bg-muted"><ExternalLink className="h-4 w-4" />{t("fileSharingOpenLink", "Open link")}</button>
                </div>
              </div>
              <PublicAccessCard
                state={publicState}
                publicUrl={snapshot.publicShareUrl ?? null}
                error={snapshot.publicError ?? null}
                passwordProtected={Boolean(snapshot.passwordProtected)}
                busy={loading}
                missingDependency={environment?.installed === false}
                copied={copied}
                onEnable={() => void enablePublic()}
                onDisable={() => void disablePublic()}
                onRetry={() => void enablePublic()}
                onCopy={() => void copyPublicLink()}
              />
              <p className="text-sm text-muted-foreground">{t("fileSharingStarted", "Started")}: {snapshot.startedAt ? new Date(snapshot.startedAt).toLocaleString() : "-"}</p>
            </div>
          </div>
          <section><h3 className="mb-2 text-sm font-semibold">{t("fileSharingFiles", "Shared files")} ({snapshot.files.length})</h3><div className="max-h-48 overflow-y-auto rounded-md border">{snapshot.files.map((file) => <div key={file.id} className="flex items-center justify-between gap-4 border-b p-3 text-sm last:border-0"><span className="min-w-0 truncate" title={file.name}>{file.name}</span><span className="shrink-0 text-muted-foreground">{formatBytes(file.size)}</span></div>)}</div></section>
          <section><h3 className="mb-2 text-sm font-semibold">{t("fileSharingTransfers", "Transfers")}</h3><div className="max-h-48 overflow-y-auto rounded-md border">{snapshot.transfers.length ? snapshot.transfers.map((transfer) => <div key={transfer.id} className="grid grid-cols-[minmax(0,1fr)_auto] gap-4 border-b p-3 text-sm last:border-0"><span className="truncate">{transfer.fileName} · {transfer.clientAddress}</span><span>{t(`fileSharingState_${transfer.state}`, transfer.state)} · {formatBytes(transfer.bytesSent)}</span></div>) : <p className="p-3 text-sm text-muted-foreground">{t("fileSharingNoTransfers", "No transfers yet.")}</p>}</div></section>
          <div className="text-sm text-muted-foreground">{t("fileSharingSummary", "Completed {{completed}}, failed {{failed}}, sent {{bytes}}", { completed: snapshot.summary.completedTransfers, failed: snapshot.summary.failedTransfers + snapshot.summary.cancelledTransfers, bytes: formatBytes(snapshot.summary.bytesSent) })}</div>
          <button type="button" onClick={() => void stop()} disabled={loading} className="inline-flex h-10 w-fit items-center gap-2 rounded-md bg-destructive px-4 text-sm font-medium text-destructive-foreground disabled:opacity-50"><Square className="h-4 w-4" />{t("fileSharingStop", "Stop sharing")}</button>
          <details className="rounded-md border">
            <summary className="cursor-pointer p-3 text-sm font-semibold">{t("fileSharingStart", "Start sharing")}</summary>
            <div className="border-t p-3">{startForm}</div>
          </details>
        </div>
      ) : (
        startForm
      )}
    </div>
  );
}
