import type { TFunction } from "i18next";
import type { LucideIcon } from "lucide-react";
import { AlertCircle, Hash, Plus, RefreshCw } from "lucide-react";
import type { SshTunnelModeCard } from "../helpers/sshTunnelHelpers";

type SshTunnelsHeaderProps = {
  t: TFunction;
  toolIcon: LucideIcon;
  iconClassName: string;
  error: string | null;
  modeCards: SshTunnelModeCard[];
  onRefresh: () => void;
  onOpenCommonPorts: () => void;
  onOpenCreate: () => void;
};

export function SshTunnelsHeader({
  t,
  toolIcon: ToolIcon,
  iconClassName,
  error,
  modeCards,
  onRefresh,
  onOpenCommonPorts,
  onOpenCreate,
}: SshTunnelsHeaderProps) {
  return (
    <>
      <div className="flex items-start justify-between gap-4">
        <div className="flex items-start gap-3">
          <div className={`rounded-lg p-2 ${iconClassName}`}>
            <ToolIcon className="h-5 w-5" />
          </div>
          <div>
            <h2 className="text-xl font-bold tracking-tight">
              {t("sshTunnels", "SSH Tunnels")}
            </h2>
            <p className="mt-1 text-sm text-muted-foreground">
              {t(
                "sshTunnelsDesc",
                "Create, detect, connect, and disconnect local, remote, or dynamic SSH forwarding profiles.",
              )}
            </p>
          </div>
        </div>
        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={onRefresh}
            className="inline-flex items-center gap-2 rounded-md bg-secondary px-3 py-2 text-sm font-medium text-secondary-foreground transition-colors hover:bg-secondary/80"
          >
            <RefreshCw className="h-4 w-4" />
            {t("refresh", "Refresh")}
          </button>
          <button
            type="button"
            onClick={onOpenCommonPorts}
            className="inline-flex items-center gap-2 rounded-md border bg-background px-3 py-2 text-sm font-medium text-foreground transition-colors hover:bg-muted"
          >
            <Hash className="h-4 w-4 text-muted-foreground" />
            {t("sshTunnelCommonPorts", "Common Ports")}
          </button>
          <button
            type="button"
            onClick={onOpenCreate}
            className="inline-flex items-center gap-2 rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground transition-colors hover:bg-primary/90"
          >
            <Plus className="h-4 w-4" />
            {t("newSshTunnel", "New Tunnel")}
          </button>
        </div>
      </div>

      {error ? (
        <div className="rounded-xl border border-destructive/20 bg-destructive/10 p-4 text-sm text-destructive">
          <div className="flex items-start gap-3">
            <AlertCircle className="mt-0.5 h-4 w-4 shrink-0" />
            <div>{error}</div>
          </div>
        </div>
      ) : null}

      <div className="grid gap-3 md:grid-cols-3">
        {modeCards.map((card) => {
          const Icon = card.icon;
          return (
            <div key={card.id} className="rounded-xl border bg-card p-4 shadow-sm">
              <div className="flex items-center gap-2 text-sm font-semibold">
                <Icon className="h-4 w-4 text-primary" />
                {card.title}
              </div>
              <p className="mt-2 text-sm text-muted-foreground">{card.description}</p>
              <p className="mt-3 text-xs leading-5 text-muted-foreground/90">{card.example}</p>
            </div>
          );
        })}
      </div>
    </>
  );
}
