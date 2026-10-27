import type { Dispatch, SetStateAction } from "react";
import type { TFunction } from "i18next";
import {
  Activity,
  Hash,
  Loader2,
  MoreHorizontal,
  Pencil,
  Play,
  Unplug,
} from "lucide-react";
import type {
  SshTunnelGroupView,
  SshTunnelRuntimeView,
  SshTunnelView,
} from "../types";
import {
  ALL_CONNECTED_TAB_ID,
} from "../helpers/sshTunnelHelpers";
import type { SshTunnelGroupStatus } from "../helpers/sshTunnelHelpers";

type SshTunnelTabsProps = {
  t: TFunction;
  connectedViewTone: SshTunnelGroupStatus["tone"];
  connectedTunnels: SshTunnelView[];
  isConnectedView: boolean;
  activeGroupId: string;
  setActiveGroupId: Dispatch<SetStateAction<string>>;
  groups: SshTunnelGroupView[];
  groupStatusMap: Record<string, SshTunnelGroupStatus>;
  groupMenuOpen: boolean;
  setGroupMenuOpen: Dispatch<SetStateAction<boolean>>;
  groupBusyAction: "connect" | "disconnect" | null;
  visibleTunnels: SshTunnelView[];
  runtimeMap: Record<string, SshTunnelRuntimeView>;
  onAllDisconnect: () => void;
  onGroupBatch: (operation: "connect" | "disconnect", groupId: string) => void;
  onManageGroups: () => void;
  onManageCommonPorts: () => void;
};

export function SshTunnelTabs({
  t,
  connectedViewTone,
  connectedTunnels,
  isConnectedView,
  activeGroupId,
  setActiveGroupId,
  groups,
  groupStatusMap,
  groupMenuOpen,
  setGroupMenuOpen,
  groupBusyAction,
  visibleTunnels,
  runtimeMap,
  onAllDisconnect,
  onGroupBatch,
  onManageGroups,
  onManageCommonPorts,
}: SshTunnelTabsProps) {
  return (
    <div className="flex flex-wrap items-center gap-3">
      <div className="inline-flex w-fit items-center rounded-lg border border-black bg-white p-1">
        <button
          type="button"
          onClick={() => setActiveGroupId(ALL_CONNECTED_TAB_ID)}
          title={
            connectedViewTone
              ? t("sshTunnelConnectedViewAria", {
                  count: connectedTunnels.length,
                  defaultValue: `View all active tunnels (${connectedTunnels.length})`,
                })
              : undefined
          }
          className={`group relative inline-flex items-center gap-1.5 px-3 py-1.5 rounded-md text-sm transition-colors ${
            isConnectedView
              ? "bg-black text-white"
              : "bg-white text-black hover:bg-neutral-50"
          }`}
          data-testid="ssh-tunnel-connected-view-tab"
        >
          <Activity
            className={`h-3.5 w-3.5 transition-colors ${
              isConnectedView
                ? "text-white"
                : "text-muted-foreground group-hover:text-foreground"
            }`}
          />
          <span className="font-medium">{t("sshTunnelConnectedView", "Connected")}</span>
          <span
            className={`rounded-full px-1.5 py-0.2 text-[11px] font-semibold transition-colors ${
              isConnectedView
                ? "bg-white/20 text-white"
                : connectedTunnels.length > 0
                  ? "bg-emerald-50 text-emerald-700 border border-emerald-200/60"
                  : "bg-muted text-muted-foreground group-hover:bg-muted/80 group-hover:text-foreground"
            }`}
            data-testid="connected-view-count"
          >
            {connectedTunnels.length}
          </span>
        </button>
        <div
          className="mx-1 h-4 w-px bg-neutral-300 dark:bg-neutral-600"
          aria-hidden="true"
        />
        {groups.map((group) => {
          const label = group.is_default
            ? t("sshTunnelDefaultGroup")
            : group.name;
          const statusInfo = groupStatusMap[group.id];
          const isActive = !isConnectedView && activeGroupId === group.id;
          return (
            <button
              key={group.id}
              type="button"
              onClick={() => setActiveGroupId(group.id)}
              title={statusInfo?.tooltip || undefined}
              className={`group relative inline-flex items-center gap-1.5 px-3 py-1.5 rounded-md text-sm transition-colors ${
                isActive ? "bg-black text-white" : "bg-white text-black hover:bg-neutral-50"
              }`}
              data-testid={`ssh-tunnel-group-tab-${group.id}`}
            >
              <span>{label}</span>
              {statusInfo && statusInfo.connectedCount > 0 && (
                <span
                  className={`rounded-full px-1.5 py-0.2 text-[11px] font-semibold transition-colors ${
                    isActive
                      ? "bg-white/20 text-white"
                      : "bg-emerald-50 text-emerald-700 border border-emerald-200/60"
                  }`}
                  data-testid={`group-tab-count-${group.id}`}
                >
                  {statusInfo.connectedCount}
                </span>
              )}
              {statusInfo && statusInfo.connectedCount === 0 && statusInfo.errorCount > 0 && (
                <span
                  className={`rounded-full px-1.5 py-0.2 text-[11px] font-semibold transition-colors ${
                    isActive
                      ? "bg-white/20 text-white"
                      : "bg-destructive/10 text-destructive border border-destructive/20"
                  }`}
                  data-testid={`group-tab-count-${group.id}`}
                >
                  {statusInfo.errorCount}
                </span>
              )}
              {statusInfo && statusInfo.connectedCount === 0 && statusInfo.errorCount === 0 && statusInfo.connectingCount > 0 && (
                <span
                  className={`rounded-full px-1.5 py-0.2 text-[11px] font-semibold transition-colors ${
                    isActive
                      ? "bg-white/20 text-white"
                      : "bg-amber-50 text-amber-700 border border-amber-200/60"
                  }`}
                  data-testid={`group-tab-count-${group.id}`}
                >
                  {statusInfo.connectingCount}
                </span>
              )}
            </button>
          );
        })}
      </div>

      <div className="relative" data-group-menu-root>
        <button
          type="button"
          onClick={() => setGroupMenuOpen((prev) => !prev)}
          disabled={groupBusyAction !== null}
          aria-haspopup="menu"
          aria-expanded={groupMenuOpen}
          className="inline-flex items-center gap-1.5 rounded-md border px-2 py-1.5 text-sm font-medium transition-colors hover:bg-muted disabled:opacity-50"
        >
          {groupBusyAction ? (
            <Loader2 className="h-3.5 w-3.5 animate-spin" />
          ) : (
            <MoreHorizontal className="h-3.5 w-3.5" />
          )}
          {t("sshTunnelGroupActions")}
        </button>
        {groupMenuOpen ? (
          <div
            role="menu"
            className="absolute left-0 top-full z-20 mt-1 w-44 rounded-lg border bg-popover p-1 shadow-lg"
          >
            {isConnectedView ? (
              <button
                type="button"
                role="menuitem"
                onClick={() => {
                  setGroupMenuOpen(false);
                  void onAllDisconnect();
                }}
                disabled={visibleTunnels.filter(
                  (tunnel) =>
                    runtimeMap[tunnel.id]?.status === "connected" ||
                    runtimeMap[tunnel.id]?.status === "connecting",
                ).length === 0}
                className="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm font-medium text-destructive transition-colors hover:bg-destructive/10 disabled:opacity-50"
              >
                <Unplug className="h-3.5 w-3.5" />
                {t("sshTunnelConnectedDisconnectAll", "Disconnect all active tunnels")}
              </button>
            ) : (
              <>
                <button
                  type="button"
                  role="menuitem"
                  onClick={() => {
                    setGroupMenuOpen(false);
                    void onGroupBatch("connect", activeGroupId);
                  }}
                  disabled={visibleTunnels.filter(
                    (tunnel) =>
                      runtimeMap[tunnel.id]?.status !== "connected" &&
                      runtimeMap[tunnel.id]?.status !== "connecting",
                  ).length === 0}
                  className="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm font-medium transition-colors hover:bg-muted disabled:opacity-50"
                >
                  <Play className="h-3.5 w-3.5" />
                  {t("sshTunnelGroupConnectAll")}
                </button>
                <button
                  type="button"
                  role="menuitem"
                  onClick={() => {
                    setGroupMenuOpen(false);
                    void onGroupBatch("disconnect", activeGroupId);
                  }}
                  disabled={visibleTunnels.filter(
                    (tunnel) =>
                      runtimeMap[tunnel.id]?.status === "connected" ||
                      runtimeMap[tunnel.id]?.status === "connecting",
                  ).length === 0}
                  className="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm font-medium text-destructive transition-colors hover:bg-destructive/10 disabled:opacity-50"
                >
                  <Unplug className="h-3.5 w-3.5" />
                  {t("sshTunnelGroupDisconnectAll")}
                </button>
              </>
            )}
            <div className="my-1 border-t" />
            <button
              type="button"
              role="menuitem"
              onClick={() => {
                setGroupMenuOpen(false);
                onManageGroups();
              }}
              className="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm font-medium transition-colors hover:bg-muted"
            >
              <Pencil className="h-3.5 w-3.5" />
              {t("sshTunnelManageGroups")}
            </button>
            <button
              type="button"
              role="menuitem"
              onClick={() => {
                setGroupMenuOpen(false);
                onManageCommonPorts();
              }}
              className="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm font-medium transition-colors hover:bg-muted"
            >
              <Hash className="h-3.5 w-3.5" />
              {t("sshTunnelManageCommonPorts", "Manage Common Ports")}
            </button>
          </div>
        ) : null}
      </div>
    </div>
  );
}
