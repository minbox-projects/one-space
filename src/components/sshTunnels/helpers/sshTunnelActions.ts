import { open } from "@tauri-apps/plugin-dialog";
import type { TFunction } from "i18next";
import {
  buildConnectTunnelActionDescriptor,
  buildDeleteTunnelActionDescriptor,
  buildDisconnectTunnelActionDescriptor,
} from "@/lib/actionDescriptors/sshTunnels";
import { safeRecordMessage } from "@/lib/messages";
import {
  notifyActionResult,
  runUserAction,
} from "@/lib/userActions";
import type { ActionContext } from "@/lib/userActions";
import {
  sshCommonPortDelete,
  sshCommonPortUpsert,
  sshTunnelConnect,
  sshTunnelDelete,
  sshTunnelDisconnect,
  sshTunnelGroupConnect,
  sshTunnelGroupDelete,
  sshTunnelGroupDisconnect,
  sshTunnelGroupUpsert,
  sshTunnelProbeDraft,
  sshTunnelProbeSaved,
  sshTunnelsDisconnectAll,
  sshTunnelUpsert,
} from "@/lib/sshTunnels";
import { DEFAULT_TUNNEL_GROUP_ID } from "../types";
import type {
  SshCommonPortUpsertInput,
  SshCommonPortView,
  SshTunnelBatchOperationResult,
  SshTunnelGroupView,
  SshTunnelProbeResult,
  SshTunnelRuntimeView,
  SshTunnelView,
} from "../types";
import {
  ensureDefaultGroup,
  normalizeTunnel,
  SAVED_PROBE_TTL_MS,
  sortTunnelGroups,
} from "./sshTunnelHelpers";
import type { SshTunnelsData } from "../hooks/useSshTunnelsData";

type ConfirmDialogFn = (
  message: string,
  options?: {
    title?: string;
    okLabel?: string;
    cancelLabel?: string;
    kind?: "info" | "warning" | "error";
  },
) => Promise<boolean>;

type SshTunnelActionsParams = {
  data: SshTunnelsData;
  actionContext: ActionContext;
  t: TFunction;
  pushToast: ActionContext["pushToast"];
  confirmDialog: ConfirmDialogFn;
};

export function createSshTunnelActions({
  data,
  actionContext,
  t,
  pushToast,
  confirmDialog,
}: SshTunnelActionsParams) {
  const {
    isTauri,
    form,
    tunnels,
    runtimeMap,
    visibleTunnels,
    setGroups,
    setTunnels,
    setActiveGroupId,
    setRuntimeMap,
    setError,
    setForm,
    setEditorOpen,
    setCommonPorts,
    setCommonPortSubmitting,
    setSaving,
    setGroupSubmitting,
    setDraftProbe,
    setSavedProbeMap,
    setBusyAction,
    setGroupBusyAction,
    setOpenActionMenuId,
    hydrateForm,
    resetEditor,
    buildPayload,
    loadData,
    notify,
    formatTunnelError,
    recordTunnelMessage,
    formatProbeMessage,
  } = data;

  const openCreateEditor = () => {
    setOpenActionMenuId(null);
    hydrateForm(null);
    setEditorOpen(true);
  };

  const openEditEditor = (tunnel: SshTunnelView) => {
    setOpenActionMenuId(null);
    hydrateForm(tunnel);
    setEditorOpen(true);
  };

  const handleCreateGroup = async (name: string) => {
    if (!isTauri) return;
    try {
      setGroupSubmitting(true);
      setError(null);
      const created = await runUserAction(
        actionContext,
        {
          source: "ssh_tunnels",
          category: "save",
          action: "create-group",
          target: { tab: "ssh-tunnels" },
          dedupeKey: `ssh-tunnels:create-group:${name.trim()}`,
          success: {
            title: t("sshTunnelGroupCreated", "Group created"),
            summary: t("sshTunnelGroupCreatedSummary", "Tunnel group created successfully."),
          },
          error: {
            title: t("sshTunnelGroupCreateFailed", "Failed to create tunnel group"),
          },
        },
        () => sshTunnelGroupUpsert<SshTunnelGroupView>({ name: name.trim() }),
      );
      if (!created) return;
      setGroups((prev) => sortTunnelGroups(ensureDefaultGroup([...prev, created])));
      void loadData();
    } catch (err) {
      const text = formatTunnelError(err);
      setError(text);
      await notify(text);
      throw err;
    } finally {
      setGroupSubmitting(false);
    }
  };

  const handleRenameGroup = async (group: SshTunnelGroupView, name: string) => {
    if (!isTauri) return;
    try {
      setGroupSubmitting(true);
      setError(null);
      const updated = await runUserAction(
        actionContext,
        {
          source: "ssh_tunnels",
          category: "save",
          action: "rename-group",
          target: { tab: "ssh-tunnels", entity_id: group.id },
          dedupeKey: `ssh-tunnels:rename-group:${group.id}`,
          success: {
            title: t("sshTunnelGroupRenamed", "Group updated"),
            summary: t("sshTunnelGroupRenamedSummary", "Tunnel group updated successfully."),
          },
          error: {
            title: t("sshTunnelGroupRenameFailed", "Failed to update tunnel group"),
          },
        },
        () => sshTunnelGroupUpsert<SshTunnelGroupView>({ id: group.id, name: name.trim() }),
      );
      if (!updated) return;
      setGroups((prev) =>
        sortTunnelGroups(
          ensureDefaultGroup(prev.map((item) => (item.id === updated.id ? updated : item))),
        ),
      );
      void loadData();
    } catch (err) {
      const text = formatTunnelError(err);
      setError(text);
      await notify(text);
      throw err;
    } finally {
      setGroupSubmitting(false);
    }
  };

  const handleDeleteGroup = async (group: SshTunnelGroupView) => {
    const confirmed = await confirmDialog(
      t(
        "sshTunnelDeleteGroupConfirm",
        'Delete environment group "{{name}}"? Tunnels in this group will move to the default group.',
        { name: group.name },
      ),
      {
        okLabel: t("delete", "Delete"),
        cancelLabel: t("cancel", "Cancel"),
      },
    );
    if (!confirmed || !isTauri) return;
    try {
      setGroupSubmitting(true);
      setError(null);
      await runUserAction(
        actionContext,
        {
          source: "ssh_tunnels",
          category: "delete",
          action: "delete-group",
          target: { tab: "ssh-tunnels", entity_id: group.id },
          dedupeKey: `ssh-tunnels:delete-group:${group.id}`,
          success: {
            title: t("sshTunnelGroupDeleted", "Group deleted"),
            summary: t("sshTunnelGroupDeletedSummary", "Tunnel group deleted successfully."),
          },
          error: {
            title: t("sshTunnelGroupDeleteFailed", "Failed to delete tunnel group"),
          },
        },
        () => sshTunnelGroupDelete(group.id),
      );
      setGroups((prev) => prev.filter((item) => item.id !== group.id));
      setTunnels((prev) =>
        prev.map((item) =>
          item.group_id === group.id ? { ...item, group_id: DEFAULT_TUNNEL_GROUP_ID } : item,
        ),
      );
      setActiveGroupId((prev) =>
        prev === group.id ? DEFAULT_TUNNEL_GROUP_ID : prev,
      );
      void loadData();
    } catch (err) {
      const text = formatTunnelError(err);
      setError(text);
      await notify(text);
    } finally {
      setGroupSubmitting(false);
    }
  };

  const handleCreateCommonPort = async (input: SshCommonPortUpsertInput) => {
    if (!isTauri) return;
    try {
      setCommonPortSubmitting(true);
      const saved = await runUserAction(
        actionContext,
        {
          source: "ssh_tunnels",
          category: "save",
          action: "create-common-port",
          target: { tab: "ssh-tunnels", entity_id: null },
          dedupeKey: `ssh-tunnels:create-common-port:${input.port}:${input.name}`,
          success: {
            title: t("sshTunnelCommonPortCreated", "Common port added"),
            summary: t(
              "sshTunnelCommonPortCreatedSummary",
              "Common port added successfully.",
            ),
          },
          error: {
            title: t("sshTunnelCommonPortCreateFailed", "Failed to add common port"),
          },
        },
        () => sshCommonPortUpsert<SshCommonPortView>(input),
      );
      if (!saved) return;
      setCommonPorts((prev) => {
        const next = prev.filter((p) => p.id !== saved.id);
        next.push(saved);
        next.sort(
          (a, b) =>
            a.localPort - b.localPort ||
            a.remotePort - b.remotePort ||
            a.name.localeCompare(b.name),
        );
        return next;
      });
      void loadData();
    } catch (err) {
      const text = formatTunnelError(err);
      setError(text);
      await notify(text);
      throw err;
    } finally {
      setCommonPortSubmitting(false);
    }
  };

  const handleUpdateCommonPort = async (
    port: SshCommonPortView,
    input: SshCommonPortUpsertInput,
  ) => {
    if (!isTauri) return;
    try {
      setCommonPortSubmitting(true);
      const saved = await runUserAction(
        actionContext,
        {
          source: "ssh_tunnels",
          category: "save",
          action: "update-common-port",
          target: { tab: "ssh-tunnels", entity_id: port.id },
          dedupeKey: `ssh-tunnels:update-common-port:${port.id}`,
          success: {
            title: t("sshTunnelCommonPortUpdated", "Common port updated"),
            summary: t(
              "sshTunnelCommonPortUpdatedSummary",
              "Common port updated successfully.",
            ),
          },
          error: {
            title: t("sshTunnelCommonPortUpdateFailed", "Failed to update common port"),
          },
        },
        () => sshCommonPortUpsert<SshCommonPortView>(input),
      );
      if (!saved) return;
      setCommonPorts((prev) => {
        const next = prev.filter((p) => p.id !== saved.id);
        next.push(saved);
        next.sort(
          (a, b) =>
            a.localPort - b.localPort ||
            a.remotePort - b.remotePort ||
            a.name.localeCompare(b.name),
        );
        return next;
      });
      void loadData();
    } catch (err) {
      const text = formatTunnelError(err);
      setError(text);
      await notify(text);
      throw err;
    } finally {
      setCommonPortSubmitting(false);
    }
  };

  const handleDeleteCommonPort = async (port: SshCommonPortView) => {
    if (!isTauri) return;
    const confirmed = await confirmDialog(
      t(
        "sshTunnelDeleteCommonPortConfirm",
        "Are you sure you want to remove {{name}} ({{port}}) from common ports?",
        { name: port.name, port: port.port },
      ),
      {
        title: t("sshTunnelDeleteCommonPortTitle", "Delete Common Port"),
        okLabel: t("delete", "Delete"),
        cancelLabel: t("cancel", "Cancel"),
        kind: "warning",
      },
    );
    if (!confirmed) return;
    try {
      setCommonPortSubmitting(true);
      await runUserAction(
        actionContext,
        {
          source: "ssh_tunnels",
          category: "delete",
          action: "delete-common-port",
          target: { tab: "ssh-tunnels", entity_id: port.id },
          dedupeKey: `ssh-tunnels:delete-common-port:${port.id}`,
          success: {
            title: t("sshTunnelCommonPortDeleted", "Common port deleted"),
            summary: t(
              "sshTunnelCommonPortDeletedSummary",
              "Common port removed successfully.",
            ),
          },
          error: {
            title: t("sshTunnelCommonPortDeleteFailed", "Failed to delete common port"),
          },
        },
        () => sshCommonPortDelete(port.id),
      );
      setCommonPorts((prev) => prev.filter((item) => item.id !== port.id));
      void loadData();
    } catch (err) {
      const text = formatTunnelError(err);
      setError(text);
      await notify(text);
    } finally {
      setCommonPortSubmitting(false);
    }
  };

  const handleSelectCommonPort = (item: SshCommonPortView) => {
    setForm((prev) => {
      const next = { ...prev };
      if (prev.forward_mode === "local") {
        next.local_port = String(item.localPort);
        next.target_port = String(item.remotePort);
        if (!prev.name.trim()) {
          next.name = `${item.name} (${item.remotePort})`;
        }
      } else if (prev.forward_mode === "remote") {
        next.remote_port = String(item.remotePort);
        next.target_port = String(item.localPort);
        if (!prev.name.trim()) {
          next.name = `${item.name} (${item.remotePort})`;
        }
      } else if (prev.forward_mode === "dynamic") {
        next.local_port = String(item.localPort);
        if (!prev.name.trim()) {
          next.name = `${item.name} (${item.localPort})`;
        }
      }
      return next;
    });
  };

  const handlePickKey = async () => {
    try {
      const selected = await open({ multiple: false });
      if (selected && typeof selected === "string") {
        setForm((prev) => ({ ...prev, custom_key_path: selected }));
      }
    } catch (err) {
      console.error(err);
    }
  };

  const handleSave = async () => {
    if (!isTauri) return;
    try {
      setSaving(true);
      setError(null);
      const saved = await runUserAction(
        actionContext,
        {
          source: "ssh_tunnels",
          category: "save",
          action: form.id ? "update-tunnel" : "create-tunnel",
          target: { tab: "ssh-tunnels", entity_id: form.id || null },
          dedupeKey: `ssh-tunnels:save:${form.id || form.name.trim()}`,
          success: {
            title: form.id
              ? t("sshTunnelUpdated", "Tunnel updated")
              : t("sshTunnelCreated", "Tunnel created"),
            summary: form.id
              ? t("sshTunnelUpdatedSummary", "Tunnel updated successfully.")
              : t("sshTunnelCreatedSummary", "Tunnel created successfully."),
          },
          error: {
            title: t("sshTunnelSaveFailed", "Failed to save tunnel"),
          },
        },
        () => sshTunnelUpsert<SshTunnelView>(buildPayload() as Record<string, unknown>),
      );
      if (!saved) return;
      const normalizedSaved = normalizeTunnel(saved);
      setSavedProbeMap((prev) => {
        const next = { ...prev };
        delete next[normalizedSaved.id];
        return next;
      });
      setRuntimeMap((prev) => {
        const next = { ...prev };
        delete next[normalizedSaved.id];
        return next;
      });
      setTunnels((prev) => {
        const next = prev.filter((item) => item.id !== normalizedSaved.id);
        next.unshift(normalizedSaved);
        next.sort((a, b) => b.updated_at - a.updated_at);
        return next;
      });
      setActiveGroupId(normalizedSaved.group_id);
      resetEditor();
      void loadData();
    } catch (err) {
      const text = formatTunnelError(err);
      setError(text);
      await notify(text);
    } finally {
      setSaving(false);
    }
  };

  const handleDraftProbe = async () => {
    if (!isTauri) return;
    try {
      setSaving(true);
      const result = await sshTunnelProbeDraft<SshTunnelProbeResult>(
        buildPayload() as Record<string, unknown>,
      );
      setDraftProbe(result);
      if (!result.ok) {
        recordTunnelMessage(
          "probe",
          t("sshTunnelProbeFailedMessageTitle", "SSH tunnel probe failed"),
          formatProbeMessage(result),
          result.last_error || result.message,
        );
        await notify(formatProbeMessage(result));
      }
    } catch (err) {
      const text = formatTunnelError(err);
      setDraftProbe({
        ok: false,
        summary: "",
        message: text,
        last_error: text,
      });
      recordTunnelMessage(
        "probe",
        t("sshTunnelProbeFailedMessageTitle", "SSH tunnel probe failed"),
        text,
        err,
      );
      await notify(text);
    } finally {
      setSaving(false);
    }
  };

  const handleSavedProbe = async (id: string) => {
    if (!isTauri) return;
    try {
      setBusyAction({ id, kind: "probe" });
      const result = await sshTunnelProbeSaved<SshTunnelProbeResult>(id);
      setSavedProbeMap((prev) => ({
        ...prev,
        [id]: { result, expiresAt: Date.now() + SAVED_PROBE_TTL_MS },
      }));
      const message = formatProbeMessage(result);
      if (!result.ok) {
        recordTunnelMessage(
          "probe",
          t("sshTunnelProbeFailedMessageTitle", "SSH tunnel probe failed"),
          message,
          result.last_error || result.message,
          id,
        );
      }
      notify(
        message,
        result.ok ? "success" : "error",
        result.ok
          ? t("sshTunnelProbeSuccess", "Tunnel check succeeded.")
          : t("sshTunnelProbeFailed", "Tunnel check failed."),
      );
    } catch (err) {
      const text = formatTunnelError(err);
      setError(text);
      recordTunnelMessage(
        "probe",
        t("sshTunnelProbeFailedMessageTitle", "SSH tunnel probe failed"),
        text,
        err,
        id,
      );
      notify(text, "error", t("sshTunnelProbeFailed", "Tunnel check failed."));
    } finally {
      setBusyAction(null);
    }
  };

  const handleConnect = async (id: string) => {
    if (!isTauri) return;
    try {
      setBusyAction({ id, kind: "connect" });
      const runtime = await runUserAction(
        actionContext,
        buildConnectTunnelActionDescriptor(t, id),
        () => sshTunnelConnect<SshTunnelRuntimeView>(id),
      );
      if (!runtime) return;
      await loadData();
    } catch (err) {
      const text = formatTunnelError(err);
      const tunnel = tunnels.find((item) => item.id === id);
      setError(text);
      recordTunnelMessage(
        "manual-connect",
        t(
          "sshTunnelConnectFailedMessageTitle",
          "SSH tunnel connection failed",
        ),
        `${tunnel?.name || id}: ${text}`,
        err,
        id,
      );
      notify(text, "error", t("sshTunnelConnectFailed", "Failed to connect tunnel."));
      await loadData();
    } finally {
      setBusyAction(null);
    }
  };

  const handleDisconnect = async (id: string) => {
    if (!isTauri) return;
    try {
      setBusyAction({ id, kind: "disconnect" });
      const runtime = await runUserAction(
        actionContext,
        buildDisconnectTunnelActionDescriptor(t, id),
        () => sshTunnelDisconnect<SshTunnelRuntimeView>(id),
      );
      if (!runtime) return;
      await loadData();
    } catch (err) {
      const text = formatTunnelError(err);
      setError(text);
      notify(
        text,
        "error",
        t("sshTunnelDisconnectFailed", "Failed to disconnect tunnel."),
      );
      await loadData();
    } finally {
      setBusyAction(null);
    }
  };

  const handleDelete = async (tunnel: SshTunnelView) => {
    if (!isTauri) return;
    try {
      setBusyAction({ id: tunnel.id, kind: "delete" });
      await runUserAction(
        actionContext,
        buildDeleteTunnelActionDescriptor(t, {
          id: tunnel.id,
          name: tunnel.name,
        }),
        () => sshTunnelDelete(tunnel.id),
      );
      setSavedProbeMap((prev) => {
        const next = { ...prev };
        delete next[tunnel.id];
        return next;
      });
      await loadData();
    } catch (err) {
      const text = formatTunnelError(err);
      setError(text);
      await notify(text);
    } finally {
      setBusyAction(null);
    }
  };

  const handleGroupBatch = async (
    operation: "connect" | "disconnect",
    groupId: string,
  ) => {
    if (!isTauri) return;

    const isConnect = operation === "connect";
    const candidates = visibleTunnels.filter((tunnel) => {
      const status = runtimeMap[tunnel.id]?.status;
      return isConnect
        ? status !== "connected" && status !== "connecting"
        : status === "connected" || status === "connecting";
    });

    if (candidates.length === 0) {
      pushToast({
        title: t(
          isConnect
            ? "sshTunnelGroupNoConnectable"
            : "sshTunnelGroupNoDisconnectable",
        ),
        description: t(
          isConnect ? "sshTunnelGroupConnectInfo" : "sshTunnelGroupDisconnectInfo",
        ),
        kind: "info",
      });
      return;
    }

    const keyPrefix = isConnect
      ? "sshTunnelGroupConnect"
      : "sshTunnelGroupDisconnect";
    const action = `group-${operation}`;

    try {
      setGroupBusyAction(operation);
      const result = isConnect
        ? await sshTunnelGroupConnect<SshTunnelBatchOperationResult>(groupId)
        : await sshTunnelGroupDisconnect<SshTunnelBatchOperationResult>(groupId);

      await loadData();

      if (result.failed_count === 0) {
        const skipped =
          result.skipped_count > 0
            ? t(
                isConnect
                  ? "sshTunnelGroupSkippedConnected"
                  : "sshTunnelGroupSkippedDisconnected",
                { count: result.skipped_count },
              )
            : "";
        await notifyActionResult(
          { pushToast, recordMessage: safeRecordMessage },
          {
            source: "ssh_tunnels",
            category: operation,
            action,
            target: { tab: "ssh-tunnels", entity_id: groupId },
            dedupeKey: `ssh-tunnels:${action}:${groupId}`,
          },
          "success",
          {
            title: t(`${keyPrefix}SuccessTitle`),
            summary: t(`${keyPrefix}SuccessDesc`, {
              count: result.success_count,
              group: result.group_name,
              skipped,
            }),
          },
        );
      } else {
        const failureNames = result.failures.map((f) => f.tunnel_name).join(", ");
        await notifyActionResult(
          { pushToast, recordMessage: safeRecordMessage },
          {
            source: "ssh_tunnels",
            category: operation,
            action,
            target: { tab: "ssh-tunnels", entity_id: groupId },
            dedupeKey: `ssh-tunnels:${action}:${groupId}:partial`,
          },
          "error",
          {
            title: t(`${keyPrefix}PartialTitle`),
            summary: t(`${keyPrefix}PartialDesc`, {
              success: result.success_count,
              failed: result.failed_count,
              names: failureNames,
            }),
          },
        );
      }
    } catch (err) {
      const text = formatTunnelError(err);
      setError(text);
      pushToast({
        title: text,
        description: t(
          isConnect ? "sshTunnelGroupConnectFailed" : "sshTunnelGroupDisconnectFailed",
        ),
        kind: "error",
      });
    } finally {
      setGroupBusyAction(null);
    }
  };

  const handleAllDisconnect = async () => {
    if (!isTauri) return;

    const candidates = tunnels.filter((tunnel) => {
      const status = runtimeMap[tunnel.id]?.status;
      return status === "connected" || status === "connecting";
    });

    if (candidates.length === 0) {
      pushToast({
        title: t("sshTunnelGroupNoDisconnectable"),
        description: t("sshTunnelGroupDisconnectInfo"),
        kind: "info",
      });
      return;
    }

    try {
      setGroupBusyAction("disconnect");
      const result = await sshTunnelsDisconnectAll<SshTunnelBatchOperationResult>();
      await loadData();

      if (result.failed_count === 0) {
        const skipped =
          result.skipped_count > 0
            ? t("sshTunnelGroupSkippedDisconnected", { count: result.skipped_count })
            : "";
        await notifyActionResult(
          { pushToast, recordMessage: safeRecordMessage },
          {
            source: "ssh_tunnels",
            category: "disconnect",
            action: "tunnels-disconnect-all",
            target: { tab: "ssh-tunnels", entity_id: "all" },
            dedupeKey: "ssh-tunnels:disconnect:all",
          },
          "success",
          {
            title: t("sshTunnelAllDisconnectSuccessTitle"),
            summary: t("sshTunnelAllDisconnectSuccessDesc", {
              count: result.success_count,
              skipped,
            }),
          },
        );
      } else {
        const failureNames = result.failures.map((f) => f.tunnel_name).join(", ");
        await notifyActionResult(
          { pushToast, recordMessage: safeRecordMessage },
          {
            source: "ssh_tunnels",
            category: "disconnect",
            action: "tunnels-disconnect-all",
            target: { tab: "ssh-tunnels", entity_id: "all" },
            dedupeKey: "ssh-tunnels:disconnect:all:partial",
          },
          "error",
          {
            title: t("sshTunnelGroupDisconnectPartialTitle"),
            summary: t("sshTunnelGroupDisconnectPartialDesc", {
              success: result.success_count,
              failed: result.failed_count,
              names: failureNames,
            }),
          },
        );
      }
    } catch (err) {
      const text = formatTunnelError(err);
      setError(text);
      pushToast({
        title: text,
        description: t("sshTunnelGroupDisconnectFailed"),
        kind: "error",
      });
    } finally {
      setGroupBusyAction(null);
    }
  };

  return {
    openCreateEditor,
    openEditEditor,
    handleCreateGroup,
    handleRenameGroup,
    handleDeleteGroup,
    handleCreateCommonPort,
    handleUpdateCommonPort,
    handleDeleteCommonPort,
    handleSelectCommonPort,
    handlePickKey,
    handleSave,
    handleDraftProbe,
    handleSavedProbe,
    handleConnect,
    handleDisconnect,
    handleDelete,
    handleGroupBatch,
    handleAllDisconnect,
  };
}
