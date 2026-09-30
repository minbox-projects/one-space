import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Hash, Loader2, Pencil, Plus, Search, Trash2 } from "lucide-react";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "../ui/dialog";
import type { SshCommonPortUpsertInput, SshCommonPortView } from "./types";

type SshCommonPortManagerDialogProps = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  ports: SshCommonPortView[];
  submitting: boolean;
  onCreate: (input: SshCommonPortUpsertInput) => Promise<void> | void;
  onUpdate: (port: SshCommonPortView, input: SshCommonPortUpsertInput) => Promise<void> | void;
  onDelete: (port: SshCommonPortView) => Promise<void> | void;
};

export function SshCommonPortManagerDialog({
  open,
  onOpenChange,
  ports,
  submitting,
  onCreate,
  onUpdate,
  onDelete,
}: SshCommonPortManagerDialogProps) {
  const { t } = useTranslation();

  // Create form state
  const [newLocalPort, setNewLocalPort] = useState("");
  const [newRemotePort, setNewRemotePort] = useState("");
  const [newName, setNewName] = useState("");
  const [newDescription, setNewDescription] = useState("");
  const [createError, setCreateError] = useState<string | null>(null);

  // Edit form state
  const [editingPortId, setEditingPortId] = useState<string | null>(null);
  const [editingLocalPort, setEditingLocalPort] = useState("");
  const [editingRemotePort, setEditingRemotePort] = useState("");
  const [editingName, setEditingName] = useState("");
  const [editingDescription, setEditingDescription] = useState("");
  const [editError, setEditError] = useState<string | null>(null);

  // Filter query
  const [searchQuery, setSearchQuery] = useState("");

  const editingItem = useMemo(
    () => ports.find((item) => item.id === editingPortId) || null,
    [ports, editingPortId],
  );

  const filteredPorts = useMemo(() => {
    const query = searchQuery.trim().toLowerCase();
    if (!query) return ports;
    return ports.filter((item) => {
      const localPortStr = String(item.localPort);
      const remotePortStr = String(item.remotePort);
      const nameMatch = item.name.toLowerCase().includes(query);
      const descMatch = (item.description || "").toLowerCase().includes(query);
      return (
        localPortStr.includes(query) ||
        remotePortStr.includes(query) ||
        nameMatch ||
        descMatch
      );
    });
  }, [ports, searchQuery]);

  const handleOpenChange = (nextOpen: boolean) => {
    if (!nextOpen) {
      setNewLocalPort("");
      setNewRemotePort("");
      setNewName("");
      setNewDescription("");
      setCreateError(null);
      setEditingPortId(null);
      setEditingLocalPort("");
      setEditingRemotePort("");
      setEditingName("");
      setEditingDescription("");
      setEditError(null);
      setSearchQuery("");
    }
    onOpenChange(nextOpen);
  };

  const handleNewLocalPortChange = (value: string) => {
    setNewLocalPort(value);
    if (!newRemotePort || newRemotePort === newLocalPort) {
      setNewRemotePort(value);
    }
  };

  const beginEdit = (item: SshCommonPortView) => {
    setEditingPortId(item.id);
    setEditingLocalPort(String(item.localPort));
    setEditingRemotePort(String(item.remotePort));
    setEditingName(item.name);
    setEditingDescription(item.description || "");
    setEditError(null);
  };

  const cancelEdit = () => {
    setEditingPortId(null);
    setEditingLocalPort("");
    setEditingRemotePort("");
    setEditingName("");
    setEditingDescription("");
    setEditError(null);
  };

  const submitCreate = async () => {
    setCreateError(null);
    const parsedLocal = Number.parseInt(newLocalPort.trim(), 10);
    if (!Number.isFinite(parsedLocal) || parsedLocal < 1 || parsedLocal > 65535) {
      setCreateError(t("sshTunnelPortInvalidRange", "Port must be between 1 and 65535"));
      return;
    }
    const parsedRemote = Number.parseInt(newRemotePort.trim(), 10);
    if (!Number.isFinite(parsedRemote) || parsedRemote < 1 || parsedRemote > 65535) {
      setCreateError(t("sshTunnelPortInvalidRange", "Port must be between 1 and 65535"));
      return;
    }
    const trimmedName = newName.trim();
    if (!trimmedName) {
      setCreateError(t("sshTunnelCommonPortNameRequired", "Service name cannot be empty"));
      return;
    }
    try {
      await onCreate({
        localPort: parsedLocal,
        remotePort: parsedRemote,
        name: trimmedName,
        description: newDescription.trim() || undefined,
      });
      setNewLocalPort("");
      setNewRemotePort("");
      setNewName("");
      setNewDescription("");
      setCreateError(null);
    } catch {
      // Error handled by caller
    }
  };

  const submitUpdate = async () => {
    if (!editingItem) return;
    setEditError(null);
    const parsedLocal = Number.parseInt(editingLocalPort.trim(), 10);
    if (!Number.isFinite(parsedLocal) || parsedLocal < 1 || parsedLocal > 65535) {
      setEditError(t("sshTunnelPortInvalidRange", "Port must be between 1 and 65535"));
      return;
    }
    const parsedRemote = Number.parseInt(editingRemotePort.trim(), 10);
    if (!Number.isFinite(parsedRemote) || parsedRemote < 1 || parsedRemote > 65535) {
      setEditError(t("sshTunnelPortInvalidRange", "Port must be between 1 and 65535"));
      return;
    }
    const trimmedName = editingName.trim();
    if (!trimmedName) {
      setEditError(t("sshTunnelCommonPortNameRequired", "Service name cannot be empty"));
      return;
    }
    try {
      await onUpdate(editingItem, {
        id: editingItem.id,
        localPort: parsedLocal,
        remotePort: parsedRemote,
        name: trimmedName,
        description: editingDescription.trim() || undefined,
      });
      cancelEdit();
    } catch {
      // Error handled by caller
    }
  };

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      {open ? (
        <DialogContent className="max-w-2xl max-h-[85vh] flex flex-col p-0">
          <DialogHeader className="border-b px-6 pt-6 pb-4">
            <DialogTitle>{t("sshTunnelManageCommonPorts", "Manage Common Ports")}</DialogTitle>
            <DialogDescription>
              {t(
                "sshTunnelManageCommonPortsDesc",
                "Configure reusable ports and service labels for quick selection when creating SSH tunnels.",
              )}
            </DialogDescription>
          </DialogHeader>

          <div className="flex-1 overflow-y-auto px-6 py-4 space-y-4">
            {/* Create new common port card */}
            <div className="rounded-xl border bg-muted/20 p-4">
              <div className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
                {t("sshTunnelAddCommonPort", "Add Common Port")}
              </div>
              <div className="mt-3 grid gap-2 md:grid-cols-[110px_110px_1fr_1fr_auto]">
                <input
                  type="number"
                  min={1}
                  max={65535}
                  value={newLocalPort}
                  onChange={(event) => handleNewLocalPortChange(event.target.value)}
                  placeholder={t("sshTunnelLocalPortShort", "Local Port")}
                  title={t("sshTunnelLocalPort", "Local Port")}
                  className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
                />
                <input
                  type="number"
                  min={1}
                  max={65535}
                  value={newRemotePort}
                  onChange={(event) => setNewRemotePort(event.target.value)}
                  placeholder={t("sshTunnelRemotePortShort", "Target Port")}
                  title={t("targetPort", "Target Port")}
                  className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
                />
                <input
                  type="text"
                  value={newName}
                  onChange={(event) => setNewName(event.target.value)}
                  placeholder={t("sshTunnelServiceNamePlaceholder", "Service name (e.g. MySQL)")}
                  className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
                />
                <input
                  type="text"
                  value={newDescription}
                  onChange={(event) => setNewDescription(event.target.value)}
                  placeholder={t("description", "Description (optional)")}
                  className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
                />
                <button
                  type="button"
                  onClick={() => void submitCreate()}
                  disabled={submitting}
                  className="inline-flex shrink-0 items-center justify-center gap-1.5 rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground transition-colors hover:bg-primary/90 disabled:opacity-60"
                >
                  {submitting ? (
                    <Loader2 className="h-4 w-4 animate-spin" />
                  ) : (
                    <Plus className="h-4 w-4" />
                  )}
                  {t("add", "Add")}
                </button>
              </div>
              {createError && (
                <div className="mt-2 text-xs text-destructive">{createError}</div>
              )}
            </div>

            {/* Search filter if more than 5 ports */}
            {ports.length > 5 && (
              <div className="relative">
                <Search className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-muted-foreground" />
                <input
                  type="text"
                  value={searchQuery}
                  onChange={(e) => setSearchQuery(e.target.value)}
                  placeholder={t("sshTunnelSearchCommonPorts", "Filter ports or services...")}
                  className="flex h-9 w-full rounded-md border border-input bg-background pl-9 pr-3 text-sm"
                />
              </div>
            )}

            {/* List of common ports */}
            <div className="space-y-2">
              {filteredPorts.length === 0 ? (
                <div className="py-6 text-center text-sm text-muted-foreground">
                  {searchQuery
                    ? t("sshTunnelNoMatchingPorts", "No matching ports found.")
                    : t("sshTunnelCommonPortsEmpty", "No common ports yet. Add one above.")}
                </div>
              ) : (
                filteredPorts.map((item) => {
                  const isEditing = editingPortId === item.id;
                  return (
                    <div
                      key={item.id}
                      className="flex flex-col gap-3 rounded-xl border bg-card p-3.5 transition-colors hover:bg-accent/10 md:flex-row md:items-center md:justify-between"
                    >
                      <div className="min-w-0 flex-1">
                        {isEditing ? (
                          <div className="space-y-2">
                            <div className="grid gap-2 md:grid-cols-[110px_110px_1fr_1fr]">
                              <input
                                type="number"
                                min={1}
                                max={65535}
                                value={editingLocalPort}
                                onChange={(event) => setEditingLocalPort(event.target.value)}
                                placeholder={t("sshTunnelLocalPortShort", "Local Port")}
                                title={t("sshTunnelLocalPort", "Local Port")}
                                className="flex h-9 w-full rounded-md border border-input bg-background px-3 py-1 text-sm"
                              />
                              <input
                                type="number"
                                min={1}
                                max={65535}
                                value={editingRemotePort}
                                onChange={(event) => setEditingRemotePort(event.target.value)}
                                placeholder={t("sshTunnelRemotePortShort", "Target Port")}
                                title={t("targetPort", "Target Port")}
                                className="flex h-9 w-full rounded-md border border-input bg-background px-3 py-1 text-sm"
                              />
                              <input
                                type="text"
                                value={editingName}
                                onChange={(event) => setEditingName(event.target.value)}
                                placeholder={t("name", "Name")}
                                className="flex h-9 w-full rounded-md border border-input bg-background px-3 py-1 text-sm"
                              />
                              <input
                                type="text"
                                value={editingDescription}
                                onChange={(event) => setEditingDescription(event.target.value)}
                                placeholder={t("description", "Description")}
                                className="flex h-9 w-full rounded-md border border-input bg-background px-3 py-1 text-sm"
                              />
                            </div>
                            {editError && (
                              <div className="text-xs text-destructive">{editError}</div>
                            )}
                            <div className="flex gap-2">
                              <button
                                type="button"
                                onClick={() => void submitUpdate()}
                                disabled={submitting}
                                className="rounded-md bg-primary px-3 py-1 text-xs font-medium text-primary-foreground transition-colors hover:bg-primary/90 disabled:opacity-60"
                              >
                                {t("save", "Save")}
                              </button>
                              <button
                                type="button"
                                onClick={cancelEdit}
                                disabled={submitting}
                                className="rounded-md border px-3 py-1 text-xs font-medium transition-colors hover:bg-muted disabled:opacity-60"
                              >
                                {t("cancel", "Cancel")}
                              </button>
                            </div>
                          </div>
                        ) : (
                          <div className="flex items-center gap-3">
                            <span className="inline-flex items-center gap-1 font-mono font-bold text-xs bg-primary/10 text-primary px-2.5 py-1 rounded-md shrink-0">
                              <Hash className="h-3 w-3" />
                              {item.localPort} → {item.remotePort}
                            </span>
                            <div className="min-w-0">
                              <div className="text-sm font-semibold text-foreground flex items-center gap-2">
                                <span>{item.name}</span>
                              </div>
                              {item.description ? (
                                <p className="text-xs text-muted-foreground truncate">
                                  {item.description}
                                </p>
                              ) : null}
                            </div>
                          </div>
                        )}
                      </div>

                      {!isEditing && (
                        <div className="flex shrink-0 gap-1.5">
                          <button
                            type="button"
                            onClick={() => beginEdit(item)}
                            disabled={submitting}
                            className="inline-flex items-center gap-1.5 rounded-md border px-2.5 py-1.5 text-xs font-medium transition-colors hover:bg-muted disabled:opacity-60"
                            title={t("edit", "Edit")}
                          >
                            <Pencil className="h-3.5 w-3.5" />
                            <span>{t("edit", "Edit")}</span>
                          </button>
                          <button
                            type="button"
                            onClick={() => void onDelete(item)}
                            disabled={submitting}
                            className="inline-flex items-center gap-1.5 rounded-md border border-destructive/20 px-2.5 py-1.5 text-xs font-medium text-destructive transition-colors hover:bg-destructive/10 disabled:opacity-60"
                            title={t("delete", "Delete")}
                          >
                            <Trash2 className="h-3.5 w-3.5" />
                            <span>{t("delete", "Delete")}</span>
                          </button>
                        </div>
                      )}
                    </div>
                  );
                })
              )}
            </div>
          </div>

          <DialogFooter className="border-t px-6 py-4">
            <button
              type="button"
              onClick={() => onOpenChange(false)}
              className="rounded-md border px-4 py-2 text-sm font-medium transition-colors hover:bg-muted"
            >
              {t("close", "Close")}
            </button>
          </DialogFooter>
        </DialogContent>
      ) : null}
    </Dialog>
  );
}
