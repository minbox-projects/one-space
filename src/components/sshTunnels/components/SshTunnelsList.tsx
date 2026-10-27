import type { Dispatch, ReactElement, SetStateAction } from "react";
import type { TFunction } from "i18next";
import { ChevronRight, Layers, Loader2, Plus } from "lucide-react";
import { ToolEmptyState } from "../../toolbox/ToolEmptyState";
import type { SshTunnelView } from "../types";
import type { SshTunnelConnectedSection } from "../helpers/sshTunnelHelpers";

type SshTunnelsListProps = {
  t: TFunction;
  loading: boolean;
  visibleTunnels: SshTunnelView[];
  isConnectedView: boolean;
  tunnels: SshTunnelView[];
  connectedGroupSections: SshTunnelConnectedSection[];
  setActiveGroupId: Dispatch<SetStateAction<string>>;
  onOpenCreate: () => void;
  renderTunnelCard: (tunnel: SshTunnelView) => ReactElement;
};

export function SshTunnelsList({
  t,
  loading,
  visibleTunnels,
  isConnectedView,
  tunnels,
  connectedGroupSections,
  setActiveGroupId,
  onOpenCreate,
  renderTunnelCard,
}: SshTunnelsListProps) {
  return loading ? (
    <div className="flex h-full items-center justify-center text-sm text-muted-foreground">
      <Loader2 className="mr-2 h-4 w-4 animate-spin" />
      {t("loading", "Loading...")}
    </div>
  ) : (
    <div className="min-h-0 flex-1 overflow-y-auto pr-1">
      {visibleTunnels.length === 0 ? (
        <div className="space-y-4">
          <ToolEmptyState
            title={
              isConnectedView
                ? t("sshTunnelConnectedEmpty", "No active or connected SSH tunnels currently.")
                : tunnels.length === 0
                  ? t("sshTunnelEmpty")
                  : t("sshTunnelEmptyForGroup")
            }
          />
          {!isConnectedView && (
            <div className="flex justify-center">
              <button
                type="button"
                onClick={onOpenCreate}
                className="inline-flex items-center gap-2 rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground transition-colors hover:bg-primary/90"
              >
                <Plus className="h-4 w-4" />
                {t("newSshTunnel")}
              </button>
            </div>
          )}
        </div>
      ) : isConnectedView ? (
        <div className="space-y-6">
          {connectedGroupSections.map(({ group, tunnels: groupTunnels }) => {
            const groupLabel = group.is_default
              ? t("sshTunnelDefaultGroup")
              : group.name;
            return (
              <section
                key={group.id}
                data-testid={`ssh-tunnel-connected-group-${group.id}`}
                className="rounded-2xl border bg-muted/20 p-4 transition-colors dark:bg-muted/10"
              >
                <div className="mb-3.5 flex items-center justify-between gap-3">
                  <div className="flex items-center gap-2.5">
                    <div className="flex h-7 w-7 items-center justify-center rounded-lg border bg-background shadow-xs">
                      <Layers className="h-3.5 w-3.5 text-muted-foreground" />
                    </div>
                    <button
                      type="button"
                      onClick={() => setActiveGroupId(group.id)}
                      className="group/title inline-flex items-center gap-1.5 text-left text-sm font-semibold tracking-tight text-foreground transition-colors hover:text-primary focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
                      title={t("sshTunnelSwitchToGroup", {
                        name: groupLabel,
                        defaultValue: `Switch to ${groupLabel}`,
                      })}
                    >
                      <span>{groupLabel}</span>
                    </button>
                    <span
                      className="inline-flex items-center gap-1.5 rounded-full border border-emerald-200/60 bg-emerald-50 px-2 py-0.5 text-xs font-semibold text-emerald-700 dark:border-emerald-800/60 dark:bg-emerald-950/40 dark:text-emerald-300"
                      title={t("sshTunnelGroupStatusTooltipConnected", {
                        connected: groupTunnels.length,
                        defaultValue: `${groupTunnels.length} connected`,
                      })}
                    >
                      <span className="h-1.5 w-1.5 rounded-full bg-emerald-500" />
                      <span>{groupTunnels.length}</span>
                    </span>
                  </div>
                  <button
                    type="button"
                    onClick={() => setActiveGroupId(group.id)}
                    className="group/btn inline-flex items-center gap-1 rounded-md px-2 py-1 text-xs font-medium text-muted-foreground transition-colors hover:bg-background/80 hover:text-foreground"
                    title={t("sshTunnelSwitchToGroup", {
                      name: groupLabel,
                      defaultValue: `Switch to ${groupLabel}`,
                    })}
                  >
                    <span>{t("sshTunnelViewAllInGroup", "View all")}</span>
                    <ChevronRight className="h-3.5 w-3.5 transition-transform group-hover/btn:translate-x-0.5" />
                  </button>
                </div>
                <div className="space-y-3">
                  {groupTunnels.map(renderTunnelCard)}
                </div>
              </section>
            );
          })}
        </div>
      ) : (
        <div className="space-y-4">
          {visibleTunnels.map(renderTunnelCard)}
        </div>
      )}
    </div>
  );
}
