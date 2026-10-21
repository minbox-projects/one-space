import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  Globe,
  Plus,
  Play,
  Square,
  Copy,
  Check,
  ExternalLink,
  QrCode,
  Settings2,
  Trash2,
  RefreshCw,
  Radio,
} from "lucide-react";
import { ToolShell } from "@/components/toolbox/ToolShell";
import { ToolEmptyState } from "@/components/toolbox/ToolEmptyState";
import { ToolStatusBadge } from "@/components/toolbox/ToolStatusBadge";
import { Button } from "@/components/ui/button";
import { useToast } from "@/components/ToastProvider";
import { useCopyToClipboard } from "@/toolbox/useCopyToClipboard";
import { useTauriEvent } from "@/toolbox/useTauriEvent";
import { getToolboxTool } from "@/toolbox/registry";
import { openExternalUrl } from "@/lib/externalActions";
import {
  tunnelList,
  tunnelListStatus,
  tunnelSave,
  tunnelDelete,
  tunnelStart,
  tunnelStop,
  tunnelStatus,
  tunnelGetLogs,
  tunnelCheckEnvironment,
  TUNNEL_STATUS_CHANGED_EVENT,
  type TunnelConfig,
  type TunnelRuntimeStatus,
  type EnvironmentStatus,
} from "@/lib/portTunnel";

import { EnvironmentBanner } from "./EnvironmentBanner";
import { QrCodeModal } from "./QrCodeModal";
import { TunnelConfigDialog } from "./TunnelConfigDialog";
import { LogViewer } from "./LogViewer";

export function PortTunnelTool({ isVisible = true }: { isVisible?: boolean }) {
  const { t } = useTranslation();
  const { pushToast } = useToast();
  const { iconClassName } = getToolboxTool("port-tunnel")!;
  const isTauri = "__TAURI_INTERNALS__" in window;

  const [tunnels, setTunnels] = useState<TunnelConfig[]>([]);
  const [activeTunnelId, setActiveTunnelId] = useState<string>("");
  const [runtimeStatus, setRuntimeStatus] = useState<Record<string, TunnelRuntimeStatus>>({});
  const [envStatus, setEnvStatus] = useState<EnvironmentStatus | null>(null);
  const [logs, setLogs] = useState<string[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Dialog states
  const [configDialogOpen, setConfigDialogOpen] = useState(false);
  const [editingConfig, setEditingConfig] = useState<TunnelConfig | null>(null);
  const [qrModalOpen, setQrModalOpen] = useState(false);

  const { copied, copy } = useCopyToClipboard({
    onError: () =>
      pushToast({
        title: "Could not copy link",
        kind: "error",
      }),
  });

  const activeTunnel = tunnels.find((t) => t.id === activeTunnelId) || tunnels[0];
  const activeStatus: TunnelRuntimeStatus = (activeTunnel && runtimeStatus[activeTunnel.id]) || {
    id: activeTunnel?.id || "",
    state: "stopped",
    public_url: undefined,
    error: undefined,
    started_at: undefined,
    active_connections: 0,
  };

  const isRunning = activeStatus.state === "running";
  const isStarting = activeStatus.state === "starting";

  // Refresh all tunnels and their real-time runtime statuses
  const refreshAll = useCallback(async () => {
    if (!isTauri) return;
    try {
      const [list, statusList] = await Promise.all([
        tunnelList(),
        tunnelListStatus().catch(() => []),
      ]);
      setTunnels(list);

      const statusMap: Record<string, TunnelRuntimeStatus> = {};
      for (const st of statusList) {
        statusMap[st.id] = st;
      }
      setRuntimeStatus(statusMap);

      setActiveTunnelId((current) => {
        // If current is still in list
        if (current && list.some((t) => t.id === current)) {
          // If current is already running, keep it
          if (statusMap[current]?.state === "running") return current;
          // If current is not running, but another tunnel is running, prefer focusing on the running one
          const runningTunnel = list.find((t) => statusMap[t.id]?.state === "running");
          if (runningTunnel) return runningTunnel.id;
          return current;
        }
        // If current is invalid or unset, prefer running tunnel, otherwise first tunnel
        const runningTunnel = list.find((t) => statusMap[t.id]?.state === "running");
        return runningTunnel ? runningTunnel.id : (list[0]?.id || "");
      });
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  }, [isTauri]);

  // Check environment for current active tunnel
  const refreshEnvironment = useCallback(async () => {
    if (!isTauri || !activeTunnel) return;
    try {
      const env = await tunnelCheckEnvironment(activeTunnel.mode);
      setEnvStatus(env);
    } catch (err) {
      console.warn("Failed to check environment", err);
    }
  }, [isTauri, activeTunnel]);

  // Refresh runtime status for a specific tunnel
  const refreshStatus = useCallback(async () => {
    if (!isTauri || !activeTunnel) return;
    try {
      const st = await tunnelStatus(activeTunnel.id);
      setRuntimeStatus((prev) => ({ ...prev, [activeTunnel.id]: st }));
    } catch (err) {
      console.warn("Failed to query status", err);
    }
  }, [isTauri, activeTunnel]);

  // Fetch logs
  const fetchLogs = useCallback(async () => {
    if (!isTauri || !activeTunnel) return;
    try {
      const latestLogs = await tunnelGetLogs(activeTunnel.id);
      setLogs(latestLogs);
    } catch (err) {
      console.warn("Failed to fetch logs", err);
    }
  }, [isTauri, activeTunnel]);

  // Refresh data whenever component mounts or becomes visible
  useEffect(() => {
    if (isVisible) {
      void refreshAll();
    }
  }, [isVisible, refreshAll]);

  useEffect(() => {
    if (activeTunnel) {
      refreshEnvironment();
      refreshStatus();
      fetchLogs();
    }
  }, [activeTunnel, refreshEnvironment, refreshStatus, fetchLogs]);

  // Listen to status events from backend
  useTauriEvent(TUNNEL_STATUS_CHANGED_EVENT, (payload) => {
    if (payload && typeof payload === "object" && "id" in payload) {
      const st = payload as TunnelRuntimeStatus;
      setRuntimeStatus((prev) => ({ ...prev, [st.id]: st }));
      if (st.id === activeTunnelId) {
        fetchLogs();
      }
    }
  });

  // Polling logs while running
  useEffect(() => {
    if (!isVisible || !isRunning) return;
    const interval = setInterval(() => {
      fetchLogs();
    }, 2000);
    return () => clearInterval(interval);
  }, [isVisible, isRunning, fetchLogs]);

  const handleStart = async () => {
    if (!activeTunnel) return;
    setLoading(true);
    setError(null);
    try {
      const nextStatus = await tunnelStart(activeTunnel.id);
      setRuntimeStatus((prev) => ({ ...prev, [activeTunnel.id]: nextStatus }));
      fetchLogs();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  };

  const handleStop = async () => {
    if (!activeTunnel) return;
    setLoading(true);
    setError(null);
    try {
      const nextStatus = await tunnelStop(activeTunnel.id);
      setRuntimeStatus((prev) => ({ ...prev, [activeTunnel.id]: nextStatus }));
      fetchLogs();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  };

  const handleSaveConfig = async (config: TunnelConfig) => {
    const saved = await tunnelSave(config);
    await refreshAll();
    setActiveTunnelId(saved.id);
    pushToast({
      title: t("saved", "Saved"),
      kind: "success",
    });
  };

  const handleDelete = async (id: string) => {
    if (!window.confirm(t("portTunnelDeleteConfirm", { name: activeTunnel?.name || id }))) {
      return;
    }
    try {
      await tunnelDelete(id);
      await refreshAll();
      pushToast({
        title: t("deleted", "Deleted"),
        kind: "info",
      });
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  };

  return (
    <ToolShell
      titleId="port-tunnel-title"
      title={t("portTunnel", "Port Tunnel")}
      description={t("portTunnelDesc", "Map local development ports to the public internet.")}
      icon={
        <div className={`rounded-lg p-2.5 ${iconClassName || "bg-emerald-500/10 text-emerald-600"}`}>
          <Globe className="h-6 w-6" />
        </div>
      }
      error={error}
      actions={
        <div className="flex items-center gap-2">
          <Button
            variant="outline"
            size="sm"
            onClick={() => {
              setEditingConfig(null);
              setConfigDialogOpen(true);
            }}
            className="gap-1.5 shadow-sm"
          >
            <Plus className="h-4 w-4" />
            <span>{t("portTunnelCreate", "New Tunnel")}</span>
          </Button>

          <Button
            variant="ghost"
            size="sm"
            onClick={() => {
              void refreshAll();
              refreshEnvironment();
              fetchLogs();
            }}
            className="h-8 w-8 p-0 text-muted-foreground"
          >
            <RefreshCw className="h-4 w-4" />
          </Button>
        </div>
      }
    >
      <div className="space-y-4">
        {/* Environment check guidance */}
        <EnvironmentBanner status={envStatus} />

        {tunnels.length === 0 ? (
          <div className="space-y-3">
            <ToolEmptyState
              title={t("portTunnelNoTunnels", "No port tunnels configured yet.")}
              description={t("portTunnelDesc", "Map local development ports to the public internet.")}
            />
            <div className="flex justify-center">
              <Button
                variant="default"
                size="sm"
                onClick={() => {
                  setEditingConfig(null);
                  setConfigDialogOpen(true);
                }}
                className="gap-1.5"
              >
                <Plus className="h-4 w-4" />
                <span>{t("portTunnelCreate", "New Tunnel")}</span>
              </Button>
            </div>
          </div>
        ) : (
          <>
            {/* Tunnel tabs selector */}
            {tunnels.length > 0 && (
              <div className="flex flex-wrap items-center gap-1.5 border-b border-border/50 pb-2.5">
                {tunnels.map((tunnel) => {
                  const st = runtimeStatus[tunnel.id];
                  const active = tunnel.id === activeTunnelId;
                  const running = st?.state === "running";
                  const starting = st?.state === "starting";
                  const hasError = st?.state === "error";
                  return (
                    <button
                      key={tunnel.id}
                      type="button"
                      onClick={() => setActiveTunnelId(tunnel.id)}
                      data-testid={`port-tunnel-item-${tunnel.id}`}
                      className={`flex items-center gap-2 rounded-lg px-3 py-1.5 text-xs font-medium transition-all ${
                        active
                          ? "bg-primary text-primary-foreground shadow-sm"
                          : "bg-muted/40 text-muted-foreground hover:bg-muted/80 hover:text-foreground"
                      }`}
                    >
                      <span className="relative flex h-2 w-2">
                        {running && (
                          <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-emerald-400 opacity-75" />
                        )}
                        {starting && (
                          <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-amber-400 opacity-75" />
                        )}
                        <span
                          className={`relative inline-flex rounded-full h-2 w-2 ${
                            running
                              ? "bg-emerald-500"
                              : starting
                              ? "bg-amber-500"
                              : hasError
                              ? "bg-destructive"
                              : "bg-muted-foreground/40"
                          }`}
                        />
                      </span>
                      <span>{tunnel.name}</span>
                      <span className="opacity-70 font-mono">:{tunnel.local_port}</span>
                    </button>
                  );
                })}
              </div>
            )}

            {/* Active tunnel hero control card */}
            {activeTunnel && (
              <div className="rounded-2xl border border-border/80 bg-gradient-to-b from-card to-card/60 p-5 shadow-sm space-y-5">
                {/* Header row */}
                <div className="flex flex-wrap items-center justify-between gap-3">
                  <div className="flex items-center gap-3">
                    <div className="flex items-center gap-2">
                      <span className="relative flex h-2.5 w-2.5">
                        {isRunning && (
                          <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-emerald-400 opacity-75" />
                        )}
                        {isStarting && (
                          <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-amber-400 opacity-75" />
                        )}
                        <span
                          className={`relative inline-flex rounded-full h-2.5 w-2.5 ${
                            isRunning
                              ? "bg-emerald-500"
                              : isStarting
                              ? "bg-amber-500"
                              : activeStatus.state === "error"
                              ? "bg-destructive"
                              : "bg-muted-foreground/40"
                          }`}
                        />
                      </span>
                      <h3 className="text-base font-bold text-foreground">
                        {activeTunnel.name}
                      </h3>
                    </div>

                    <div className="flex items-center gap-1.5">
                      <span className="rounded-md bg-muted px-2 py-0.5 font-mono text-xs text-muted-foreground">
                        127.0.0.1:{activeTunnel.local_port}
                      </span>
                      <span className="rounded-md border border-border/60 bg-background/50 px-2 py-0.5 text-[11px] font-medium uppercase tracking-wider text-muted-foreground">
                        {activeTunnel.mode === "cloudflare" ? "Cloudflare" : "FRP"}
                      </span>
                    </div>
                  </div>

                  <div className="flex items-center gap-2">
                    <ToolStatusBadge
                      tone={
                        isRunning
                          ? "success"
                          : isStarting
                          ? "warning"
                          : activeStatus.state === "error"
                          ? "error"
                          : "neutral"
                      }
                      label={
                        isRunning
                          ? t("portTunnelStatusRunning", "Running")
                          : isStarting
                          ? t("portTunnelStatusStarting", "Starting...")
                          : activeStatus.state === "error"
                          ? t("portTunnelStatusError", "Error")
                          : t("portTunnelStatusStopped", "Stopped")
                      }
                    />

                    <Button
                      variant="ghost"
                      size="sm"
                      onClick={() => {
                        setEditingConfig(activeTunnel);
                        setConfigDialogOpen(true);
                      }}
                      className="h-8 w-8 p-0 text-muted-foreground hover:text-foreground"
                      title={t("edit", "Edit")}
                    >
                      <Settings2 className="h-4 w-4" />
                    </Button>

                    <Button
                      variant="ghost"
                      size="sm"
                      onClick={() => handleDelete(activeTunnel.id)}
                      className="h-8 w-8 p-0 text-muted-foreground hover:text-destructive"
                      title={t("delete", "Delete")}
                    >
                      <Trash2 className="h-4 w-4" />
                    </Button>
                  </div>
                </div>

                {/* Hero Public URL Box */}
                {isRunning && activeStatus.public_url ? (
                  <div className="rounded-xl border border-emerald-500/30 bg-emerald-500/5 p-4 space-y-3 shadow-inner">
                    <div className="flex items-center justify-between text-xs text-emerald-600 dark:text-emerald-400 font-medium">
                      <div className="flex items-center gap-1.5">
                        <Radio className="h-4 w-4 animate-pulse text-emerald-500" />
                        <span>{t("portTunnelPublicUrl", "Public Access URL")}</span>
                      </div>
                      <span className="text-[11px] opacity-75">HTTPS / Global CDN</span>
                    </div>

                    <div className="flex flex-wrap items-center justify-between gap-3 rounded-lg border border-emerald-500/20 bg-background/80 p-3 shadow-sm backdrop-blur-sm">
                      <span className="font-mono text-sm font-semibold tracking-tight text-emerald-600 dark:text-emerald-400 select-all break-all">
                        {activeStatus.public_url}
                      </span>

                      <div className="flex items-center gap-1.5 shrink-0">
                        <Button
                          variant="secondary"
                          size="sm"
                          onClick={() => copy(activeStatus.public_url!)}
                          className="h-8 gap-1.5 text-xs shadow-none border border-border/40"
                        >
                          {copied ? (
                            <>
                              <Check className="h-3.5 w-3.5 text-emerald-500" />
                              <span className="text-emerald-600 dark:text-emerald-400 font-medium">
                                {t("portTunnelCopied", "Copied")}
                              </span>
                            </>
                          ) : (
                            <>
                              <Copy className="h-3.5 w-3.5" />
                              <span>{t("portTunnelCopyUrl", "Copy URL")}</span>
                            </>
                          )}
                        </Button>

                        <Button
                          variant="outline"
                          size="sm"
                          onClick={() => {
                            if (activeStatus.public_url) {
                              void openExternalUrl(activeStatus.public_url);
                            }
                          }}
                          className="h-8 gap-1.5 text-xs shadow-none"
                          title={t("portTunnelOpenBrowser", "Open in Browser")}
                        >
                          <ExternalLink className="h-3.5 w-3.5" />
                          <span>{t("open", "Open")}</span>
                        </Button>

                        <Button
                          variant="outline"
                          size="sm"
                          onClick={() => setQrModalOpen(true)}
                          className="h-8 gap-1.5 text-xs shadow-none"
                          title={t("portTunnelScanQr", "Mobile QR Code")}
                        >
                          <QrCode className="h-3.5 w-3.5 text-emerald-500" />
                          <span>QR</span>
                        </Button>
                      </div>
                    </div>
                  </div>
                ) : (
                  <div className="rounded-xl border border-dashed border-border/80 bg-muted/20 p-6 flex flex-col items-center justify-center text-center space-y-2">
                    <Globe className="h-8 w-8 text-muted-foreground/40" />
                    <div>
                      <p className="text-xs font-medium text-foreground">
                        {activeTunnel.mode === "cloudflare"
                          ? "Ready to expose port " + activeTunnel.local_port + " via Cloudflare Quick Tunnel"
                          : "Ready to connect to " + (activeTunnel.frp_server || "FRP server")}
                      </p>
                      <p className="text-[11px] text-muted-foreground mt-0.5">
                        Click Start to generate a public internet URL accessible from anywhere.
                      </p>
                    </div>
                  </div>
                )}

                {/* Primary Action Button */}
                <div className="flex items-center justify-between pt-1">
                  <div className="text-xs text-muted-foreground font-mono">
                    {activeStatus.error && (
                      <span className="text-destructive font-sans">{activeStatus.error}</span>
                    )}
                  </div>

                  <div>
                    {isRunning ? (
                      <Button
                        variant="destructive"
                        size="sm"
                        onClick={handleStop}
                        disabled={loading}
                        className="gap-2 shadow-sm font-medium"
                      >
                        <Square className="h-4 w-4 fill-current" />
                        <span>{t("portTunnelStop", "Stop Tunnel")}</span>
                      </Button>
                    ) : (
                      <Button
                        variant="default"
                        size="sm"
                        onClick={handleStart}
                        disabled={loading || isStarting}
                        className="gap-2 bg-emerald-600 hover:bg-emerald-700 text-white shadow-sm font-medium"
                      >
                        <Play className="h-4 w-4 fill-current" />
                        <span>
                          {isStarting
                            ? t("portTunnelStatusStarting", "Starting...")
                            : t("portTunnelStart", "Start Tunnel")}
                        </span>
                      </Button>
                    )}
                  </div>
                </div>
              </div>
            )}

            {/* Real-time process logs */}
            {activeTunnel && (
              <LogViewer
                logs={logs}
                onClear={() => setLogs([])}
              />
            )}
          </>
        )}
      </div>

      {/* QR Code Modal */}
      {activeStatus.public_url && (
        <QrCodeModal
          open={qrModalOpen}
          onOpenChange={setQrModalOpen}
          url={activeStatus.public_url}
          tunnelName={activeTunnel?.name || "Port Tunnel"}
        />
      )}

      {/* Edit / Create Dialog */}
      <TunnelConfigDialog
        open={configDialogOpen}
        onOpenChange={setConfigDialogOpen}
        config={editingConfig}
        onSave={handleSaveConfig}
      />
    </ToolShell>
  );
}
