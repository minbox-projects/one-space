import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import { useConfirmDialog } from "./ConfirmDialogProvider";
import { useToast } from "./ToastProvider";
import { SshTunnelGroupManagerDialog } from "./sshTunnels/SshTunnelGroupManagerDialog";
import { SshCommonPortManagerDialog } from "./sshTunnels/SshCommonPortManagerDialog";
import { safeRecordMessage } from "@/lib/messages";
import { getToolboxTool } from "@/toolbox/registry";
import type { SshTunnelView } from "./sshTunnels/types";
import { useSshTunnelsData } from "./sshTunnels/hooks/useSshTunnelsData";
import { createSshTunnelActions } from "./sshTunnels/helpers/sshTunnelActions";
import { SshTunnelsHeader } from "./sshTunnels/components/SshTunnelsHeader";
import { SshTunnelTabs } from "./sshTunnels/components/SshTunnelTabs";
import { SshTunnelsList } from "./sshTunnels/components/SshTunnelsList";
import { TunnelCard } from "./sshTunnels/components/TunnelCard";
import { TunnelEditorDialog } from "./sshTunnels/components/TunnelEditorDialog";

export function SshTunnels({
  isVisible = true,
  initialTab,
  navigationNonce,
}: {
  isVisible?: boolean;
  initialTab?: string;
  navigationNonce?: number;
}) {
  const { t } = useTranslation();
  const confirmDialog = useConfirmDialog();
  const { pushToast } = useToast();
  const { icon: ToolIcon, iconClassName } = getToolboxTool("ssh-tunnels")!;
  const actionContext = useMemo(
    () => ({
      t,
      confirm: confirmDialog,
      pushToast,
      recordMessage: safeRecordMessage,
    }),
    [confirmDialog, pushToast, t],
  );
  const data = useSshTunnelsData({
    isVisible,
    initialTab,
    navigationNonce,
    t,
    pushToast,
  });
  const actions = createSshTunnelActions({
    data,
    actionContext,
    t,
    pushToast,
    confirmDialog,
  });

  const renderTunnelCard = (tunnel: SshTunnelView) => (
    <TunnelCard
      key={tunnel.id}
      t={t}
      tunnel={tunnel}
      runtime={data.runtimeMap[tunnel.id]}
      probe={data.getSavedProbe(tunnel.id)}
      busyAction={data.busyAction}
      groupBusyAction={data.groupBusyAction}
      openActionMenuId={data.openActionMenuId}
      setOpenActionMenuId={data.setOpenActionMenuId}
      getBusyOverlayLabel={data.getBusyOverlayLabel}
      getGroupLabel={data.getGroupLabel}
      getStatusLabel={data.getStatusLabel}
      formatProbeMessage={data.formatProbeMessage}
      formatTunnelError={data.formatTunnelError}
      onConnect={(id) => {
        void actions.handleConnect(id);
      }}
      onDisconnect={(id) => {
        void actions.handleDisconnect(id);
      }}
      onProbe={(id) => {
        void actions.handleSavedProbe(id);
      }}
      onEdit={actions.openEditEditor}
      onDelete={(tunnelToDelete) => {
        void actions.handleDelete(tunnelToDelete);
      }}
    />
  );

  return (
    <div className="flex h-full flex-col space-y-6">
      <SshTunnelsHeader
        t={t}
        toolIcon={ToolIcon}
        iconClassName={iconClassName}
        error={data.error}
        modeCards={data.modeCards}
        onRefresh={() => void data.loadData()}
        onOpenCommonPorts={() => data.setCommonPortManagerOpen(true)}
        onOpenCreate={actions.openCreateEditor}
      />

      <SshTunnelTabs
        t={t}
        connectedViewTone={data.connectedViewTone}
        connectedTunnels={data.connectedTunnels}
        isConnectedView={data.isConnectedView}
        activeGroupId={data.activeGroupId}
        setActiveGroupId={data.setActiveGroupId}
        groups={data.groups}
        groupStatusMap={data.groupStatusMap}
        groupMenuOpen={data.groupMenuOpen}
        setGroupMenuOpen={data.setGroupMenuOpen}
        groupBusyAction={data.groupBusyAction}
        visibleTunnels={data.visibleTunnels}
        runtimeMap={data.runtimeMap}
        onAllDisconnect={() => void actions.handleAllDisconnect()}
        onGroupBatch={(operation, groupId) => {
          void actions.handleGroupBatch(operation, groupId);
        }}
        onManageGroups={() => data.setGroupManagerOpen(true)}
        onManageCommonPorts={() => data.setCommonPortManagerOpen(true)}
      />

      <SshTunnelsList
        t={t}
        loading={data.loading}
        visibleTunnels={data.visibleTunnels}
        isConnectedView={data.isConnectedView}
        tunnels={data.tunnels}
        connectedGroupSections={data.connectedGroupSections}
        setActiveGroupId={data.setActiveGroupId}
        onOpenCreate={actions.openCreateEditor}
        renderTunnelCard={renderTunnelCard}
      />

      <SshTunnelGroupManagerDialog
        open={data.groupManagerOpen}
        onOpenChange={data.setGroupManagerOpen}
        groups={data.groups}
        submitting={data.groupSubmitting}
        onCreate={actions.handleCreateGroup}
        onRename={actions.handleRenameGroup}
        onDelete={actions.handleDeleteGroup}
      />

      <SshCommonPortManagerDialog
        open={data.commonPortManagerOpen}
        onOpenChange={data.setCommonPortManagerOpen}
        ports={data.commonPorts}
        submitting={data.commonPortSubmitting}
        onCreate={actions.handleCreateCommonPort}
        onUpdate={actions.handleUpdateCommonPort}
        onDelete={actions.handleDeleteCommonPort}
      />

      <TunnelEditorDialog
        t={t}
        open={data.editorOpen}
        onOpenChange={(open) => {
          if (!open) {
            data.resetEditor();
            return;
          }
          data.setEditorOpen(true);
        }}
        form={data.form}
        setForm={data.setForm}
        groups={data.groups}
        hosts={data.hosts}
        modeCards={data.modeCards}
        commonPorts={data.commonPorts}
        draftProbe={data.draftProbe}
        saving={data.saving}
        onReset={data.resetEditor}
        onSave={() => void actions.handleSave()}
        onDraftProbe={() => void actions.handleDraftProbe()}
        onPickKey={() => void actions.handlePickKey()}
        onSelectCommonPort={actions.handleSelectCommonPort}
        onManageCommonPorts={() => data.setCommonPortManagerOpen(true)}
      />
    </div>
  );
}
