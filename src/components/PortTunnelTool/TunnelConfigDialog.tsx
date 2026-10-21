import { useState, useEffect } from "react";
import { useTranslation } from "react-i18next";
import { Globe, Server, Check } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import type { TunnelConfig, TunnelMode, TunnelProtocol } from "@/lib/portTunnel";

interface TunnelConfigDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  config: TunnelConfig | null;
  onSave: (config: TunnelConfig) => Promise<void>;
}

export function TunnelConfigDialog({
  open,
  onOpenChange,
  config,
  onSave,
}: TunnelConfigDialogProps) {
  const { t } = useTranslation();
  const [name, setName] = useState("");
  const [mode, setMode] = useState<TunnelMode>("cloudflare");
  const [localPort, setLocalPort] = useState<number>(3000);
  const [protocol, setProtocol] = useState<TunnelProtocol>("http");
  const [remotePort, setRemotePort] = useState<number | undefined>();
  const [frpServer, setFrpServer] = useState("");
  const [frpToken, setFrpToken] = useState("");
  const [customDomain, setCustomDomain] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (config) {
      setName(config.name);
      setMode(config.mode);
      setLocalPort(config.local_port);
      setProtocol(config.protocol);
      setRemotePort(config.remote_port);
      setFrpServer(config.frp_server || "");
      setFrpToken(config.frp_token || "");
      setCustomDomain(config.custom_domain || "");
    } else {
      setName("Web Dev 3000");
      setMode("cloudflare");
      setLocalPort(3000);
      setProtocol("http");
      setRemotePort(undefined);
      setFrpServer("");
      setFrpToken("");
      setCustomDomain("");
    }
    setError(null);
  }, [config, open]);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!name.trim()) {
      setError("Please provide a name for this tunnel");
      return;
    }
    if (!localPort || localPort < 1 || localPort > 65535) {
      setError("Local port must be between 1 and 65535");
      return;
    }

    if (mode === "frp") {
      if (!frpServer.trim()) {
        setError("FRP server address is required");
        return;
      }
      if (!customDomain.trim() && (!remotePort || remotePort <= 0)) {
        setError("Please specify a custom domain or a remote port for FRP");
        return;
      }
    }

    setSaving(true);
    setError(null);
    try {
      const payload: TunnelConfig = {
        id: config?.id || `tunnel-${Date.now()}`,
        name: name.trim(),
        mode,
        local_port: Number(localPort),
        protocol,
        remote_port: remotePort ? Number(remotePort) : undefined,
        frp_server: frpServer.trim() ? frpServer.trim() : undefined,
        frp_token: frpToken.trim() ? frpToken.trim() : undefined,
        custom_domain: customDomain.trim() ? customDomain.trim() : undefined,
        auto_start: config?.auto_start ?? false,
      };

      await onSave(payload);
      onOpenChange(false);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setSaving(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-lg">
        <form onSubmit={handleSubmit}>
          <DialogHeader>
            <DialogTitle className="text-base font-semibold">
              {config ? t("portTunnelEdit", "Edit Tunnel") : t("portTunnelCreate", "New Tunnel")}
            </DialogTitle>
          </DialogHeader>

          <div className="space-y-4 py-3 text-sm">
            {error && (
              <div className="rounded-lg bg-destructive/10 p-2.5 text-xs text-destructive">
                {error}
              </div>
            )}

            {/* Mode selection cards */}
            <div className="space-y-1.5">
              <label className="text-xs font-medium text-muted-foreground">
                {t("portTunnelMode", "Tunnel Mode")}
              </label>
              <div className="grid grid-cols-2 gap-2">
                <button
                  type="button"
                  onClick={() => setMode("cloudflare")}
                  className={`flex flex-col items-start gap-1 rounded-xl border p-3 text-left transition-all ${
                    mode === "cloudflare"
                      ? "border-emerald-500/50 bg-emerald-500/5 ring-1 ring-emerald-500/30"
                      : "border-border/60 hover:bg-muted/50"
                  }`}
                >
                  <div className="flex w-full items-center justify-between">
                    <div className="flex items-center gap-1.5 font-medium text-xs">
                      <Globe className="h-4 w-4 text-emerald-500" />
                      <span>Cloudflare Quick</span>
                    </div>
                    {mode === "cloudflare" && <Check className="h-3.5 w-3.5 text-emerald-500" />}
                  </div>
                  <span className="text-[11px] text-muted-foreground">
                    Free, instant HTTPS, zero config.
                  </span>
                </button>

                <button
                  type="button"
                  onClick={() => setMode("frp")}
                  className={`flex flex-col items-start gap-1 rounded-xl border p-3 text-left transition-all ${
                    mode === "frp"
                      ? "border-emerald-500/50 bg-emerald-500/5 ring-1 ring-emerald-500/30"
                      : "border-border/60 hover:bg-muted/50"
                  }`}
                >
                  <div className="flex w-full items-center justify-between">
                    <div className="flex items-center gap-1.5 font-medium text-xs">
                      <Server className="h-4 w-4 text-sky-500" />
                      <span>Custom FRP</span>
                    </div>
                    {mode === "frp" && <Check className="h-3.5 w-3.5 text-emerald-500" />}
                  </div>
                  <span className="text-[11px] text-muted-foreground">
                    Private server, fixed domains.
                  </span>
                </button>
              </div>
            </div>

            {/* Basic inputs */}
            <div className="grid grid-cols-2 gap-3">
              <div className="space-y-1">
                <label className="text-xs font-medium text-muted-foreground">
                  {t("portTunnelName", "Tunnel Name")}
                </label>
                <input
                  type="text"
                  value={name}
                  onChange={(e) => setName(e.target.value)}
                  placeholder="e.g. Next.js App"
                  className="w-full rounded-lg border border-input bg-background px-3 py-1.5 text-sm outline-none focus:ring-1 focus:ring-ring"
                />
              </div>

              <div className="space-y-1">
                <label className="text-xs font-medium text-muted-foreground">
                  {t("portTunnelLocalPort", "Local Port")}
                </label>
                <input
                  type="number"
                  min={1}
                  max={65535}
                  value={localPort || ""}
                  onChange={(e) => setLocalPort(Number(e.target.value))}
                  placeholder="3000"
                  className="w-full rounded-lg border border-input bg-background px-3 py-1.5 text-sm font-mono outline-none focus:ring-1 focus:ring-ring"
                />
              </div>
            </div>

            {/* FRP specific inputs */}
            {mode === "frp" && (
              <div className="space-y-3 rounded-xl border border-border/80 bg-muted/20 p-3">
                <div className="grid grid-cols-2 gap-3">
                  <div className="space-y-1">
                    <label className="text-xs font-medium text-muted-foreground">
                      {t("portTunnelFrpServer", "FRP Server Host")}
                    </label>
                    <input
                      type="text"
                      value={frpServer}
                      onChange={(e) => setFrpServer(e.target.value)}
                      placeholder="frp.mydomain.com"
                      className="w-full rounded-lg border border-input bg-background px-3 py-1.5 text-sm outline-none focus:ring-1 focus:ring-ring"
                    />
                  </div>

                  <div className="space-y-1">
                    <label className="text-xs font-medium text-muted-foreground">
                      {t("portTunnelFrpToken", "Auth Token")}
                    </label>
                    <input
                      type="password"
                      value={frpToken}
                      onChange={(e) => setFrpToken(e.target.value)}
                      placeholder="Optional Token"
                      className="w-full rounded-lg border border-input bg-background px-3 py-1.5 text-sm outline-none focus:ring-1 focus:ring-ring"
                    />
                  </div>
                </div>

                <div className="grid grid-cols-2 gap-3">
                  <div className="space-y-1">
                    <label className="text-xs font-medium text-muted-foreground">
                      {t("portTunnelCustomDomain", "Custom Domain / Subdomain")}
                    </label>
                    <input
                      type="text"
                      value={customDomain}
                      onChange={(e) => setCustomDomain(e.target.value)}
                      placeholder="app.mydomain.com"
                      className="w-full rounded-lg border border-input bg-background px-3 py-1.5 text-sm outline-none focus:ring-1 focus:ring-ring"
                    />
                  </div>

                  <div className="space-y-1">
                    <label className="text-xs font-medium text-muted-foreground">
                      {t("portTunnelRemotePort", "Remote Port (Optional)")}
                    </label>
                    <input
                      type="number"
                      value={remotePort || ""}
                      onChange={(e) => setRemotePort(e.target.value ? Number(e.target.value) : undefined)}
                      placeholder="e.g. 8080"
                      className="w-full rounded-lg border border-input bg-background px-3 py-1.5 text-sm font-mono outline-none focus:ring-1 focus:ring-ring"
                    />
                  </div>
                </div>
              </div>
            )}
          </div>

          <DialogFooter className="mt-4 gap-2">
            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={() => onOpenChange(false)}
            >
              {t("cancel", "Cancel")}
            </Button>
            <Button type="submit" size="sm" disabled={saving}>
              {saving ? t("saving", "Saving...") : t("save", "Save")}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
