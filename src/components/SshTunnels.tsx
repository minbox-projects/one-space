import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { message, open } from "@tauri-apps/plugin-dialog";
import { useTranslation } from "react-i18next";
import {
  Activity,
  AlertCircle,
  Globe,
  KeyRound,
  Link2,
  Loader2,
  Network,
  Pencil,
  Play,
  Plus,
  RefreshCw,
  Save,
  Server,
  ShieldCheck,
  Trash2,
  Unplug,
  Waypoints,
} from "lucide-react";
import { useConfirmDialog } from "./ConfirmDialogProvider";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "./ui/dialog";
import { Switch } from "./ui/switch";

type SshHost = {
  name: string;
  host_name: string;
  user: string;
  port: number;
};

type SshTunnelSourceKind = "saved_host" | "custom";
type SshTunnelAuthKind = "password" | "key";
type SshTunnelForwardMode = "local" | "remote" | "dynamic";
type SshTunnelStatus = "disconnected" | "connecting" | "connected" | "error";

type SshTunnelForwardConfig = {
  mode: SshTunnelForwardMode;
  local_bind_host?: string | null;
  local_port?: number | null;
  remote_bind_host?: string | null;
  remote_port?: number | null;
  target_host?: string | null;
  target_port?: number | null;
  dynamic_probe_host?: string | null;
  dynamic_probe_port?: number | null;
};

type SshTunnelCustomView = {
  host: string;
  port: number;
  user: string;
  auth_kind: SshTunnelAuthKind;
  key_path?: string | null;
  has_password: boolean;
};

type SshTunnelView = {
  id: string;
  name: string;
  source_kind: SshTunnelSourceKind;
  saved_host_name?: string | null;
  custom?: SshTunnelCustomView | null;
  forward: SshTunnelForwardConfig;
  auto_connect: boolean;
  created_at: number;
  updated_at: number;
  last_connected_at?: number | null;
  last_error?: string | null;
};

type SshTunnelRuntimeView = {
  id: string;
  status: SshTunnelStatus;
  active_client_count: number;
  mode: SshTunnelForwardMode;
  summary: string;
  resolved_server_host?: string | null;
  listening_addr?: string | null;
  last_error?: string | null;
};

type SshTunnelProbeResult = {
  ok: boolean;
  mode: SshTunnelForwardMode;
  summary: string;
  message: string;
  last_error?: string | null;
};

type TunnelFormState = {
  id?: string;
  name: string;
  source_kind: SshTunnelSourceKind;
  saved_host_name: string;
  custom_host: string;
  custom_port: string;
  custom_user: string;
  custom_auth_kind: SshTunnelAuthKind;
  custom_key_path: string;
  custom_password: string;
  preserve_password: boolean;
  forward_mode: SshTunnelForwardMode;
  local_port: string;
  remote_port: string;
  target_host: string;
  target_port: string;
  dynamic_probe_host: string;
  dynamic_probe_port: string;
  auto_connect: boolean;
};

const DEFAULT_FORM: TunnelFormState = {
  name: "",
  source_kind: "saved_host",
  saved_host_name: "",
  custom_host: "",
  custom_port: "22",
  custom_user: "root",
  custom_auth_kind: "password",
  custom_key_path: "",
  custom_password: "",
  preserve_password: false,
  forward_mode: "local",
  local_port: "5432",
  remote_port: "15432",
  target_host: "127.0.0.1",
  target_port: "5432",
  dynamic_probe_host: "",
  dynamic_probe_port: "",
  auto_connect: false,
};

function parseOptionalPort(value: string): number | null {
  const trimmed = value.trim();
  if (!trimmed) return null;
  const parsed = Number.parseInt(trimmed, 10);
  return Number.isFinite(parsed) && parsed > 0 ? parsed : null;
}

function statusBadgeClass(status: SshTunnelStatus) {
  switch (status) {
    case "connected":
      return "bg-emerald-500/12 text-emerald-600 border-emerald-500/20";
    case "connecting":
      return "bg-amber-500/12 text-amber-600 border-amber-500/20";
    case "error":
      return "bg-destructive/12 text-destructive border-destructive/20";
    default:
      return "bg-muted text-muted-foreground border-border";
  }
}

function modeShort(mode: SshTunnelForwardMode) {
  if (mode === "local") return "L";
  if (mode === "remote") return "R";
  return "D";
}

export function SshTunnels({ isVisible = true }: { isVisible?: boolean }) {
  const { t, i18n } = useTranslation();
  const confirmDialog = useConfirmDialog();
  const [hosts, setHosts] = useState<SshHost[]>([]);
  const [tunnels, setTunnels] = useState<SshTunnelView[]>([]);
  const [runtimeMap, setRuntimeMap] = useState<Record<string, SshTunnelRuntimeView>>({});
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [form, setForm] = useState<TunnelFormState>(DEFAULT_FORM);
  const [editorOpen, setEditorOpen] = useState(false);
  const [saving, setSaving] = useState(false);
  const [draftProbe, setDraftProbe] = useState<SshTunnelProbeResult | null>(null);
  const [savedProbeMap, setSavedProbeMap] = useState<Record<string, SshTunnelProbeResult>>({});
  const [busyId, setBusyId] = useState<string | null>(null);

  const isTauri = "__TAURI_INTERNALS__" in window;

  const modeCards = useMemo(
    () => [
      {
        id: "local" as const,
        icon: Link2,
        title: t("sshTunnelModeLocalTitle", "Local (-L)"),
        description: t(
          "sshTunnelModeLocalDesc",
          "Expose a remote service on a local port so local apps can connect to it.",
        ),
        example: t(
          "sshTunnelModeLocalExample",
          "Example: map remote PostgreSQL to 127.0.0.1:5432 on this device.",
        ),
      },
      {
        id: "remote" as const,
        icon: Waypoints,
        title: t("sshTunnelModeRemoteTitle", "Remote (-R)"),
        description: t(
          "sshTunnelModeRemoteDesc",
          "Expose a local service to the remote SSH server through a remote port.",
        ),
        example: t(
          "sshTunnelModeRemoteExample",
          "Example: let a remote host access your local dev server securely.",
        ),
      },
      {
        id: "dynamic" as const,
        icon: Globe,
        title: t("sshTunnelModeDynamicTitle", "Dynamic (-D)"),
        description: t(
          "sshTunnelModeDynamicDesc",
          "Create a local SOCKS5 proxy that can reach arbitrary destinations through SSH.",
        ),
        example: t(
          "sshTunnelModeDynamicExample",
          "Example: configure a browser or CLI to use 127.0.0.1:1080 as a SOCKS5 proxy.",
        ),
      },
    ],
    [t],
  );

  const hydrateForm = (tunnel?: SshTunnelView | null) => {
    if (!tunnel) {
      setForm(DEFAULT_FORM);
      setDraftProbe(null);
      return;
    }
    setForm({
      id: tunnel.id,
      name: tunnel.name,
      source_kind: tunnel.source_kind,
      saved_host_name: tunnel.saved_host_name || "",
      custom_host: tunnel.custom?.host || "",
      custom_port: String(tunnel.custom?.port ?? 22),
      custom_user: tunnel.custom?.user || "root",
      custom_auth_kind: tunnel.custom?.auth_kind || "password",
      custom_key_path: tunnel.custom?.key_path || "",
      custom_password: "",
      preserve_password: tunnel.custom?.has_password || false,
      forward_mode: tunnel.forward.mode,
      local_port: String(tunnel.forward.local_port ?? ""),
      remote_port: String(tunnel.forward.remote_port ?? ""),
      target_host: tunnel.forward.target_host || "127.0.0.1",
      target_port: String(tunnel.forward.target_port ?? ""),
      dynamic_probe_host: tunnel.forward.dynamic_probe_host || "",
      dynamic_probe_port: String(tunnel.forward.dynamic_probe_port ?? ""),
      auto_connect: tunnel.auto_connect,
    });
    setDraftProbe(null);
  };

  const resetEditor = () => {
    setForm(DEFAULT_FORM);
    setDraftProbe(null);
    setEditorOpen(false);
  };

  const buildPayload = () => ({
    id: form.id,
    name: form.name.trim(),
    source_kind: form.source_kind,
    saved_host_name:
      form.source_kind === "saved_host" ? form.saved_host_name.trim() : undefined,
    custom:
      form.source_kind === "custom"
        ? {
            host: form.custom_host.trim(),
            port: parseOptionalPort(form.custom_port) ?? 0,
            user: form.custom_user.trim(),
            auth_kind: form.custom_auth_kind,
            key_path:
              form.custom_auth_kind === "key"
                ? form.custom_key_path.trim() || undefined
                : undefined,
            password:
              form.custom_auth_kind === "password" ? form.custom_password : undefined,
            preserve_password:
              form.custom_auth_kind === "password" ? form.preserve_password : false,
          }
        : undefined,
    forward: {
      mode: form.forward_mode,
      local_port: parseOptionalPort(form.local_port),
      remote_port: parseOptionalPort(form.remote_port),
      target_host:
        form.forward_mode === "dynamic" ? undefined : form.target_host.trim() || undefined,
      target_port:
        form.forward_mode === "dynamic"
          ? undefined
          : parseOptionalPort(form.target_port),
      dynamic_probe_host:
        form.forward_mode === "dynamic"
          ? form.dynamic_probe_host.trim() || undefined
          : undefined,
      dynamic_probe_port:
        form.forward_mode === "dynamic"
          ? parseOptionalPort(form.dynamic_probe_port)
          : undefined,
    },
    auto_connect: form.auto_connect,
  });

  const refreshStatuses = async () => {
    if (!isTauri) return;
    const runtime = await invoke<SshTunnelRuntimeView[]>("ssh_tunnels_refresh_status");
    setRuntimeMap(
      runtime.reduce<Record<string, SshTunnelRuntimeView>>((acc, item) => {
        acc[item.id] = item;
        return acc;
      }, {}),
    );
  };

  const loadData = async () => {
    if (!isTauri) {
      setError(t("notInTauri", "This feature is only available inside the desktop app."));
      setLoading(false);
      return;
    }
    try {
      setLoading(true);
      setError(null);
      const [loadedHosts, loadedTunnels, runtime] = await Promise.all([
        invoke<SshHost[]>("get_ssh_hosts"),
        invoke<SshTunnelView[]>("ssh_tunnels_list"),
        invoke<SshTunnelRuntimeView[]>("ssh_tunnels_refresh_status"),
      ]);
      setHosts(loadedHosts);
      setTunnels(loadedTunnels);
      setRuntimeMap(
        runtime.reduce<Record<string, SshTunnelRuntimeView>>((acc, item) => {
          acc[item.id] = item;
          return acc;
        }, {}),
      );
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    void loadData();

    let unlistenUpdated: (() => void) | undefined;
    listen("ssh-tunnels-updated", () => {
      void loadData();
    })
      .then((fn) => {
        unlistenUpdated = fn;
      })
      .catch((eventError) => {
        console.error("Failed to subscribe ssh tunnel updates", eventError);
      });

    return () => {
      unlistenUpdated?.();
    };
  }, []);

  useEffect(() => {
    if (!isVisible || !isTauri) return;
    const timer = window.setInterval(() => {
      void refreshStatuses();
    }, 5000);
    return () => window.clearInterval(timer);
  }, [isVisible, isTauri]);

  const notify = async (text: string, kind: "error" | "info" = "error") => {
    await message(text, {
      title: t("sshTunnels", "SSH Tunnels"),
      kind,
    });
  };

  const openCreateEditor = () => {
    hydrateForm(null);
    setEditorOpen(true);
  };

  const openEditEditor = (tunnel: SshTunnelView) => {
    hydrateForm(tunnel);
    setEditorOpen(true);
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
      const saved = await invoke<SshTunnelView>("ssh_tunnel_upsert", {
        input: buildPayload(),
      });
      setSavedProbeMap((prev) => {
        const next = { ...prev };
        delete next[saved.id];
        return next;
      });
      await loadData();
      resetEditor();
    } catch (err) {
      const text = String(err);
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
      const result = await invoke<SshTunnelProbeResult>("ssh_tunnel_probe_draft", {
        input: buildPayload(),
      });
      setDraftProbe(result);
      if (!result.ok) {
        await notify(result.message);
      }
    } catch (err) {
      const text = String(err);
      setDraftProbe({
        ok: false,
        mode: form.forward_mode,
        summary: "",
        message: text,
        last_error: text,
      });
      await notify(text);
    } finally {
      setSaving(false);
    }
  };

  const handleSavedProbe = async (id: string) => {
    if (!isTauri) return;
    try {
      setBusyId(id);
      const result = await invoke<SshTunnelProbeResult>("ssh_tunnel_probe_saved", { id });
      setSavedProbeMap((prev) => ({ ...prev, [id]: result }));
      if (!result.ok) {
        await notify(result.message);
      }
    } catch (err) {
      await notify(String(err));
    } finally {
      setBusyId(null);
    }
  };

  const handleConnect = async (id: string) => {
    if (!isTauri) return;
    try {
      setBusyId(id);
      await invoke<SshTunnelRuntimeView>("ssh_tunnel_connect", { id });
      await loadData();
    } catch (err) {
      const text = String(err);
      setError(text);
      await notify(text);
      await loadData();
    } finally {
      setBusyId(null);
    }
  };

  const handleDisconnect = async (id: string) => {
    if (!isTauri) return;
    try {
      setBusyId(id);
      await invoke<SshTunnelRuntimeView>("ssh_tunnel_disconnect", { id });
      await loadData();
    } catch (err) {
      const text = String(err);
      setError(text);
      await notify(text);
    } finally {
      setBusyId(null);
    }
  };

  const handleDelete = async (tunnel: SshTunnelView) => {
    const confirmed = await confirmDialog(t("confirmDelete", { name: tunnel.name }), {
      okLabel: t("delete", "Delete"),
      cancelLabel: t("cancel", "Cancel"),
    });
    if (!confirmed || !isTauri) return;
    try {
      setBusyId(tunnel.id);
      await invoke("ssh_tunnel_delete", { id: tunnel.id });
      setSavedProbeMap((prev) => {
        const next = { ...prev };
        delete next[tunnel.id];
        return next;
      });
      await loadData();
    } catch (err) {
      const text = String(err);
      setError(text);
      await notify(text);
    } finally {
      setBusyId(null);
    }
  };

  return (
    <div className="flex h-full flex-col space-y-6">
      <div className="flex items-start justify-between gap-4">
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
        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={() => void loadData()}
            className="inline-flex items-center gap-2 rounded-md bg-secondary px-3 py-2 text-sm font-medium text-secondary-foreground transition-colors hover:bg-secondary/80"
          >
            <RefreshCw className="h-4 w-4" />
            {t("refresh", "Refresh")}
          </button>
          <button
            type="button"
            onClick={openCreateEditor}
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

      {loading ? (
        <div className="flex h-full items-center justify-center text-sm text-muted-foreground">
          <Loader2 className="mr-2 h-4 w-4 animate-spin" />
          {t("loading", "Loading...")}
        </div>
      ) : (
        <div className="min-h-0 flex-1 overflow-y-auto pr-1">
          {tunnels.length === 0 ? (
            <div className="rounded-xl border border-dashed p-8 text-center text-sm text-muted-foreground">
              <div>{t("sshTunnelEmpty", "No SSH tunnels yet. Create your first tunnel on the right.")}</div>
              <button
                type="button"
                onClick={openCreateEditor}
                className="mt-4 inline-flex items-center gap-2 rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground transition-colors hover:bg-primary/90"
              >
                <Plus className="h-4 w-4" />
                {t("newSshTunnel", "New Tunnel")}
              </button>
            </div>
          ) : (
            <div className="space-y-4">
              {tunnels.map((tunnel) => {
                const runtime = runtimeMap[tunnel.id];
                const probe = savedProbeMap[tunnel.id];
                const busy = busyId === tunnel.id;
                const status = runtime?.status || "disconnected";
                return (
                  <div
                    key={tunnel.id}
                    className="rounded-xl border bg-card p-5 shadow-sm transition-all hover:border-primary/30"
                  >
                    <div className="flex items-start justify-between gap-4">
                      <div className="flex-1">
                        <div className="flex flex-wrap items-center gap-2">
                          <span className="text-base font-semibold">{tunnel.name}</span>
                          <span
                            className={`rounded-full border px-2 py-0.5 text-[11px] font-medium uppercase tracking-[0.16em] ${statusBadgeClass(status)}`}
                          >
                            {status}
                          </span>
                          <span className="rounded-full border bg-muted px-2 py-0.5 text-[11px] font-medium">
                            {modeShort(tunnel.forward.mode)}
                          </span>
                        </div>
                        <div className="mt-2 text-sm text-muted-foreground">
                          {runtime?.summary ||
                            (i18n.language === "zh"
                              ? "等待状态刷新..."
                              : "Waiting for status refresh...")}
                        </div>
                        <div className="mt-3 flex flex-wrap gap-x-4 gap-y-1 text-xs text-muted-foreground">
                          <span>
                            {t("sshTunnelSource", "Source")}:{" "}
                            {tunnel.source_kind === "saved_host"
                              ? `${t("sshServers", "SSH Servers")} / ${tunnel.saved_host_name || "-"}`
                              : `${tunnel.custom?.user || "-"}@${tunnel.custom?.host || "-"}:${tunnel.custom?.port || 22}`}
                          </span>
                          <span>
                            {t("authMethod", "Authentication Method")}:{" "}
                            {tunnel.source_kind === "saved_host"
                              ? t("sshTunnelAuthInherited", "Inherited from SSH config")
                              : tunnel.custom?.auth_kind === "password"
                                ? t("password", "Password")
                                : t("sshKey", "SSH Key")}
                          </span>
                          <span>
                            {t("launchAtLogin", "Launch at login")}:{" "}
                            {tunnel.auto_connect ? t("yes", "Yes") : t("no", "No")}
                          </span>
                          {runtime?.resolved_server_host ? (
                            <span>
                              {t("sshTunnelResolvedServer", "Resolved SSH Server")}:{" "}
                              {runtime.resolved_server_host}
                            </span>
                          ) : null}
                          {runtime?.listening_addr ? (
                            <span>
                              {t("sshTunnelListening", "Listening")}: {runtime.listening_addr}
                            </span>
                          ) : null}
                          {runtime?.active_client_count ? (
                            <span>
                              {t("sshTunnelClients", "Clients")}:{" "}
                              {runtime.active_client_count}
                            </span>
                          ) : null}
                        </div>
                      </div>

                      <div className="flex shrink-0 flex-col gap-2">
                        <button
                          type="button"
                          onClick={() => void handleSavedProbe(tunnel.id)}
                          disabled={busy}
                          className="inline-flex items-center gap-2 rounded-md border px-3 py-2 text-sm font-medium transition-colors hover:bg-muted disabled:opacity-60"
                        >
                          {busy ? (
                            <Loader2 className="h-4 w-4 animate-spin" />
                          ) : (
                            <Activity className="h-4 w-4" />
                          )}
                          {t("sshTunnelProbe", "Detect Connection")}
                        </button>
                        {status === "connected" || status === "connecting" ? (
                          <button
                            type="button"
                            onClick={() => void handleDisconnect(tunnel.id)}
                            disabled={busy}
                            className="inline-flex items-center gap-2 rounded-md border border-amber-500/20 bg-amber-500/10 px-3 py-2 text-sm font-medium text-amber-700 transition-colors hover:bg-amber-500/15 disabled:opacity-60"
                          >
                            <Unplug className="h-4 w-4" />
                            {t("disconnect", "Disconnect")}
                          </button>
                        ) : (
                          <button
                            type="button"
                            onClick={() => void handleConnect(tunnel.id)}
                            disabled={busy}
                            className="inline-flex items-center gap-2 rounded-md bg-primary px-3 py-2 text-sm font-medium text-primary-foreground transition-colors hover:bg-primary/90 disabled:opacity-60"
                          >
                            <Play className="h-4 w-4" />
                            {t("connect", "Connect")}
                          </button>
                        )}
                        <div className="flex gap-2">
                          <button
                            type="button"
                            onClick={() => openEditEditor(tunnel)}
                            className="inline-flex flex-1 items-center justify-center gap-2 rounded-md border px-3 py-2 text-sm font-medium transition-colors hover:bg-muted"
                          >
                            <Pencil className="h-4 w-4" />
                            {t("edit", "Edit")}
                          </button>
                          <button
                            type="button"
                            onClick={() => void handleDelete(tunnel)}
                            className="inline-flex items-center justify-center rounded-md border border-destructive/20 px-3 py-2 text-sm font-medium text-destructive transition-colors hover:bg-destructive/10"
                          >
                            <Trash2 className="h-4 w-4" />
                          </button>
                        </div>
                      </div>
                    </div>

                    {probe ? (
                      <div
                        className={`mt-4 rounded-lg border px-3 py-2 text-sm ${
                          probe.ok
                            ? "border-emerald-500/20 bg-emerald-500/10 text-emerald-700"
                            : "border-destructive/20 bg-destructive/10 text-destructive"
                        }`}
                      >
                        {probe.message}
                      </div>
                    ) : runtime?.last_error || tunnel.last_error ? (
                      <div className="mt-4 rounded-lg border border-destructive/20 bg-destructive/10 px-3 py-2 text-sm text-destructive">
                        {runtime?.last_error || tunnel.last_error}
                      </div>
                    ) : null}
                  </div>
                );
              })}
            </div>
          )}
        </div>
      )}

      <Dialog
        open={editorOpen}
        onOpenChange={(open) => {
          if (!open) {
            resetEditor();
            return;
          }
          setEditorOpen(true);
        }}
      >
        {editorOpen && (
          <DialogContent className="max-w-4xl h-[85vh] max-h-[85vh] overflow-hidden p-0">
            <DialogHeader className="border-b px-6 pt-6 pb-4">
              <DialogTitle>
                {form.id
                  ? t("editSshTunnel", "Edit SSH Tunnel")
                  : t("newSshTunnel", "New Tunnel")}
              </DialogTitle>
              <DialogDescription>
                {t(
                  "sshTunnelEditorDesc",
                  "Choose a forwarding mode, define the SSH source, then test the tunnel before saving.",
                )}
              </DialogDescription>
            </DialogHeader>

            <div className="overflow-y-auto px-6 py-5">
              <div className="space-y-6">
                <div className="space-y-2">
                  <label className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
                    {t("name", "Name")}
                  </label>
                  <input
                    type="text"
                    value={form.name}
                    onChange={(event) =>
                      setForm((prev) => ({ ...prev, name: event.target.value }))
                    }
                    placeholder={t("sshTunnelNamePlaceholder", "e.g. Redis via Bastion")}
                    className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
                  />
                </div>

                <div className="space-y-3">
                  <div className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
                    {t("sshTunnelForwardMode", "Forwarding Mode")}
                  </div>
                  <div className="grid gap-3 md:grid-cols-3">
                    {modeCards.map((card) => {
                      const Icon = card.icon;
                      const active = form.forward_mode === card.id;
                      return (
                        <button
                          key={card.id}
                          type="button"
                          onClick={() =>
                            setForm((prev) => ({
                              ...prev,
                              forward_mode: card.id,
                              local_port:
                                card.id === "dynamic"
                                  ? prev.local_port || "1080"
                                  : prev.local_port,
                            }))
                          }
                          className={`rounded-xl border p-4 text-left transition-all ${
                            active
                              ? "border-primary bg-primary/5 shadow-sm"
                              : "hover:border-primary/30"
                          }`}
                        >
                          <div className="flex items-center gap-2 text-sm font-semibold">
                            <Icon className="h-4 w-4 text-primary" />
                            {card.title}
                          </div>
                          <div className="mt-2 text-sm text-muted-foreground">
                            {card.description}
                          </div>
                        </button>
                      );
                    })}
                  </div>
                </div>

                <div className="space-y-3">
                  <div className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
                    {t("sshTunnelSource", "SSH Source")}
                  </div>
                  <div className="grid gap-3 md:grid-cols-2">
                    <button
                      type="button"
                      onClick={() =>
                        setForm((prev) => ({ ...prev, source_kind: "saved_host" }))
                      }
                      className={`rounded-xl border p-4 text-left transition-all ${
                        form.source_kind === "saved_host"
                          ? "border-primary bg-primary/5 shadow-sm"
                          : "hover:border-primary/30"
                      }`}
                    >
                      <div className="flex items-center gap-2 text-sm font-semibold">
                        <Server className="h-4 w-4 text-primary" />
                        {t("sshTunnelUseSavedServer", "Use SSH Server")}
                      </div>
                      <p className="mt-2 text-sm text-muted-foreground">
                        {t(
                          "sshTunnelUseSavedServerDesc",
                          "Reuse an alias from SSH Servers and inherit host/user/key settings from your SSH config.",
                        )}
                      </p>
                    </button>
                    <button
                      type="button"
                      onClick={() =>
                        setForm((prev) => ({ ...prev, source_kind: "custom" }))
                      }
                      className={`rounded-xl border p-4 text-left transition-all ${
                        form.source_kind === "custom"
                          ? "border-primary bg-primary/5 shadow-sm"
                          : "hover:border-primary/30"
                      }`}
                    >
                      <div className="flex items-center gap-2 text-sm font-semibold">
                        <Network className="h-4 w-4 text-primary" />
                        {t("sshTunnelUseCustomServer", "Custom SSH")}
                      </div>
                      <p className="mt-2 text-sm text-muted-foreground">
                        {t(
                          "sshTunnelUseCustomServerDesc",
                          "Enter host, port, and credentials directly for a fully managed tunnel.",
                        )}
                      </p>
                    </button>
                  </div>
                </div>

                {form.source_kind === "saved_host" ? (
                  <div className="space-y-2">
                    <label className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
                      {t("sshTunnelSelectServer", "SSH Server")}
                    </label>
                    <select
                      value={form.saved_host_name}
                      onChange={(event) =>
                        setForm((prev) => ({
                          ...prev,
                          saved_host_name: event.target.value,
                        }))
                      }
                      className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
                    >
                      <option value="">
                        {t("sshTunnelSelectServerPlaceholder", "Choose an SSH server...")}
                      </option>
                      {hosts.map((host) => (
                        <option key={host.name} value={host.name}>
                          {host.name} · {host.user}@{host.host_name}:{host.port}
                        </option>
                      ))}
                    </select>
                  </div>
                ) : (
                  <div className="grid gap-4 md:grid-cols-2">
                    <div className="space-y-2 md:col-span-2">
                      <label className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
                        {t("hostOrIp", "Host or IP")}
                      </label>
                      <input
                        type="text"
                        value={form.custom_host}
                        onChange={(event) =>
                          setForm((prev) => ({ ...prev, custom_host: event.target.value }))
                        }
                        placeholder={t("hostOrIpPlaceholder", "e.g. 113.128.187.178")}
                        className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
                      />
                    </div>
                    <div className="space-y-2">
                      <label className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
                        {t("username", "Username")}
                      </label>
                      <input
                        type="text"
                        value={form.custom_user}
                        onChange={(event) =>
                          setForm((prev) => ({ ...prev, custom_user: event.target.value }))
                        }
                        className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
                      />
                    </div>
                    <div className="space-y-2">
                      <label className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
                        {t("port", "Port")}
                      </label>
                      <input
                        type="number"
                        value={form.custom_port}
                        onChange={(event) =>
                          setForm((prev) => ({ ...prev, custom_port: event.target.value }))
                        }
                        className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
                      />
                    </div>
                    <div className="space-y-2 md:col-span-2">
                      <label className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
                        {t("authMethod", "Authentication Method")}
                      </label>
                      <div className="grid gap-3 md:grid-cols-2">
                        <button
                          type="button"
                          onClick={() =>
                            setForm((prev) => ({ ...prev, custom_auth_kind: "password" }))
                          }
                          className={`rounded-xl border p-4 text-left transition-all ${
                            form.custom_auth_kind === "password"
                              ? "border-primary bg-primary/5 shadow-sm"
                              : "hover:border-primary/30"
                          }`}
                        >
                          <div className="flex items-center gap-2 text-sm font-semibold">
                            <ShieldCheck className="h-4 w-4 text-primary" />
                            {t("password", "Password")}
                          </div>
                        </button>
                        <button
                          type="button"
                          onClick={() =>
                            setForm((prev) => ({ ...prev, custom_auth_kind: "key" }))
                          }
                          className={`rounded-xl border p-4 text-left transition-all ${
                            form.custom_auth_kind === "key"
                              ? "border-primary bg-primary/5 shadow-sm"
                              : "hover:border-primary/30"
                          }`}
                        >
                          <div className="flex items-center gap-2 text-sm font-semibold">
                            <KeyRound className="h-4 w-4 text-primary" />
                            {t("sshKey", "SSH Key")}
                          </div>
                        </button>
                      </div>
                    </div>
                    {form.custom_auth_kind === "password" ? (
                      <div className="space-y-2 md:col-span-2">
                        <label className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
                          {t("password", "Password")}
                        </label>
                        <input
                          type="password"
                          value={form.custom_password}
                          onChange={(event) =>
                            setForm((prev) => ({
                              ...prev,
                              custom_password: event.target.value,
                              preserve_password: false,
                            }))
                          }
                          placeholder={
                            form.preserve_password
                              ? t(
                                  "sshTunnelPasswordSaved",
                                  "Saved password will be kept unless you type a new one.",
                                )
                              : t("sshTunnelPasswordPlaceholder", "Enter SSH password")
                          }
                          className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
                        />
                        {form.id && form.preserve_password ? (
                          <label className="inline-flex items-center gap-2 text-sm text-muted-foreground">
                            <input
                              type="checkbox"
                              checked={form.preserve_password}
                              onChange={(event) =>
                                setForm((prev) => ({
                                  ...prev,
                                  preserve_password: event.target.checked,
                                }))
                              }
                              className="h-4 w-4"
                            />
                            {t(
                              "sshTunnelKeepPassword",
                              "Keep the saved password until I enter a new one.",
                            )}
                          </label>
                        ) : null}
                      </div>
                    ) : (
                      <div className="space-y-2 md:col-span-2">
                        <label className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
                          {t("sshKeyPath", "Private Key Path")}
                        </label>
                        <div className="flex gap-2">
                          <input
                            type="text"
                            value={form.custom_key_path}
                            onChange={(event) =>
                              setForm((prev) => ({
                                ...prev,
                                custom_key_path: event.target.value,
                              }))
                            }
                            placeholder={t(
                              "sshKeyPathPlaceholder",
                              "e.g. /Users/name/.ssh/id_rsa",
                            )}
                            className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
                          />
                          <button
                            type="button"
                            onClick={() => void handlePickKey()}
                            className="rounded-md border px-3 py-2 text-sm font-medium transition-colors hover:bg-muted"
                          >
                            {t("browse", "Browse")}
                          </button>
                        </div>
                      </div>
                    )}
                  </div>
                )}

                {form.forward_mode === "local" ? (
                  <div className="grid gap-4 md:grid-cols-2">
                    <div className="space-y-2">
                      <label className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
                        {t("sshTunnelLocalPort", "Local Port")}
                      </label>
                      <input
                        type="number"
                        value={form.local_port}
                        onChange={(event) =>
                          setForm((prev) => ({ ...prev, local_port: event.target.value }))
                        }
                        className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
                      />
                    </div>
                    <div className="space-y-2">
                      <label className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
                        {t("targetHost", "Target Host")}
                      </label>
                      <input
                        type="text"
                        value={form.target_host}
                        onChange={(event) =>
                          setForm((prev) => ({ ...prev, target_host: event.target.value }))
                        }
                        className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
                      />
                    </div>
                    <div className="space-y-2">
                      <label className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
                        {t("targetPort", "Target Port")}
                      </label>
                      <input
                        type="number"
                        value={form.target_port}
                        onChange={(event) =>
                          setForm((prev) => ({ ...prev, target_port: event.target.value }))
                        }
                        className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
                      />
                    </div>
                    <div className="rounded-xl border bg-muted/40 p-4 text-sm text-muted-foreground">
                      {t(
                        "sshTunnelLocalBindHint",
                        "OneSpace always binds local and dynamic ports to 127.0.0.1 for safety.",
                      )}
                    </div>
                  </div>
                ) : null}

                {form.forward_mode === "remote" ? (
                  <div className="grid gap-4 md:grid-cols-2">
                    <div className="space-y-2">
                      <label className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
                        {t("sshTunnelRemotePort", "Remote Port")}
                      </label>
                      <input
                        type="number"
                        value={form.remote_port}
                        onChange={(event) =>
                          setForm((prev) => ({ ...prev, remote_port: event.target.value }))
                        }
                        className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
                      />
                    </div>
                    <div className="rounded-xl border bg-muted/40 p-4 text-sm text-muted-foreground">
                      {t(
                        "sshTunnelRemoteBindHint",
                        "Remote forwarding listens on 127.0.0.1 on the SSH server in v1 to avoid accidental public exposure.",
                      )}
                    </div>
                    <div className="space-y-2">
                      <label className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
                        {t("sshTunnelLocalTargetHost", "Local Target Host")}
                      </label>
                      <input
                        type="text"
                        value={form.target_host}
                        onChange={(event) =>
                          setForm((prev) => ({ ...prev, target_host: event.target.value }))
                        }
                        className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
                      />
                    </div>
                    <div className="space-y-2">
                      <label className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
                        {t("sshTunnelLocalTargetPort", "Local Target Port")}
                      </label>
                      <input
                        type="number"
                        value={form.target_port}
                        onChange={(event) =>
                          setForm((prev) => ({ ...prev, target_port: event.target.value }))
                        }
                        className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
                      />
                    </div>
                  </div>
                ) : null}

                {form.forward_mode === "dynamic" ? (
                  <div className="grid gap-4 md:grid-cols-2">
                    <div className="space-y-2">
                      <label className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
                        {t("sshTunnelSocksPort", "SOCKS5 Port")}
                      </label>
                      <input
                        type="number"
                        value={form.local_port}
                        onChange={(event) =>
                          setForm((prev) => ({ ...prev, local_port: event.target.value }))
                        }
                        className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
                      />
                    </div>
                    <div className="rounded-xl border bg-muted/40 p-4 text-sm text-muted-foreground">
                      {t(
                        "sshTunnelDynamicHint",
                        "Use the optional probe target below if you want Detect Connection to confirm that a real destination is reachable through the SOCKS5 proxy.",
                      )}
                    </div>
                    <div className="space-y-2">
                      <label className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
                        {t("sshTunnelProbeHost", "Probe Host (Optional)")}
                      </label>
                      <input
                        type="text"
                        value={form.dynamic_probe_host}
                        onChange={(event) =>
                          setForm((prev) => ({
                            ...prev,
                            dynamic_probe_host: event.target.value,
                          }))
                        }
                        placeholder="example.com"
                        className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
                      />
                    </div>
                    <div className="space-y-2">
                      <label className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
                        {t("sshTunnelProbePort", "Probe Port (Optional)")}
                      </label>
                      <input
                        type="number"
                        value={form.dynamic_probe_port}
                        onChange={(event) =>
                          setForm((prev) => ({
                            ...prev,
                            dynamic_probe_port: event.target.value,
                          }))
                        }
                        placeholder="443"
                        className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
                      />
                    </div>
                  </div>
                ) : null}

                <div className="flex items-center justify-between gap-4 rounded-xl border bg-muted/30 px-4 py-3">
                  <div className="text-sm text-muted-foreground">
                    {t(
                      "sshTunnelAutoConnect",
                      "Automatically connect this tunnel when OneSpace starts.",
                    )}
                  </div>
                  <Switch
                    checked={form.auto_connect}
                    onCheckedChange={(checked) =>
                      setForm((prev) => ({
                        ...prev,
                        auto_connect: checked,
                      }))
                    }
                    aria-label={t(
                      "sshTunnelAutoConnect",
                      "Automatically connect this tunnel when OneSpace starts.",
                    )}
                  />
                </div>

                {draftProbe ? (
                  <div
                    className={`rounded-xl border px-4 py-3 text-sm ${
                      draftProbe.ok
                        ? "border-emerald-500/20 bg-emerald-500/10 text-emerald-700"
                        : "border-destructive/20 bg-destructive/10 text-destructive"
                    }`}
                  >
                    <div className="font-medium">{draftProbe.summary}</div>
                    <div className="mt-1">{draftProbe.message}</div>
                  </div>
                ) : null}
              </div>
            </div>

            <DialogFooter className="border-t px-6 py-4">
              <button
                type="button"
                onClick={resetEditor}
                className="rounded-md border px-4 py-2 text-sm font-medium transition-colors hover:bg-muted"
              >
                {t("cancel", "Cancel")}
              </button>
              <button
                type="button"
                onClick={() => void handleDraftProbe()}
                disabled={saving}
                className="inline-flex items-center gap-2 rounded-md border px-4 py-2 text-sm font-medium transition-colors hover:bg-muted disabled:opacity-60"
              >
                {saving ? (
                  <Loader2 className="h-4 w-4 animate-spin" />
                ) : (
                  <Activity className="h-4 w-4" />
                )}
                {t("sshTunnelProbe", "Detect Connection")}
              </button>
              <button
                type="button"
                onClick={() => void handleSave()}
                disabled={saving}
                className="inline-flex items-center gap-2 rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground transition-colors hover:bg-primary/90 disabled:opacity-60"
              >
                {saving ? (
                  <Loader2 className="h-4 w-4 animate-spin" />
                ) : (
                  <Save className="h-4 w-4" />
                )}
                {t("save", "Save")}
              </button>
            </DialogFooter>
          </DialogContent>
        )}
      </Dialog>
    </div>
  );
}
