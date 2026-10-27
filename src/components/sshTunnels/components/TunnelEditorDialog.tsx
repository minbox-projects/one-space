import type { Dispatch, SetStateAction } from "react";
import type { TFunction } from "i18next";
import {
  Activity,
  KeyRound,
  Loader2,
  Network,
  Save,
  Server,
  ShieldCheck,
} from "lucide-react";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "../../ui/dialog";
import { Switch } from "../../ui/switch";
import { CommonPortSelectMenu } from "../CommonPortSelectMenu";
import type {
  SshCommonPortView,
  SshHost,
  SshTunnelGroupView,
  SshTunnelProbeResult,
  TunnelFormState,
} from "../types";
import type { SshTunnelModeCard } from "../helpers/sshTunnelHelpers";

type TunnelEditorDialogProps = {
  t: TFunction;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  form: TunnelFormState;
  setForm: Dispatch<SetStateAction<TunnelFormState>>;
  groups: SshTunnelGroupView[];
  hosts: SshHost[];
  modeCards: SshTunnelModeCard[];
  commonPorts: SshCommonPortView[];
  draftProbe: SshTunnelProbeResult | null;
  saving: boolean;
  onReset: () => void;
  onSave: () => void;
  onDraftProbe: () => void;
  onPickKey: () => void;
  onSelectCommonPort: (item: SshCommonPortView) => void;
  onManageCommonPorts: () => void;
};

export function TunnelEditorDialog({
  t,
  open,
  onOpenChange,
  form,
  setForm,
  groups,
  hosts,
  modeCards,
  commonPorts,
  draftProbe,
  saving,
  onReset,
  onSave,
  onDraftProbe,
  onPickKey,
  onSelectCommonPort,
  onManageCommonPorts,
}: TunnelEditorDialogProps) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      {open && (
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

              <div className="space-y-2">
                <label className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
                  {t("sshTunnelEnvironmentGroupOptional")}
                </label>
                <select
                  value={form.group_id}
                  onChange={(event) =>
                    setForm((prev) => ({ ...prev, group_id: event.target.value }))
                  }
                  className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
                >
                  <option value="">
                    {t("sshTunnelEnvironmentGroupDefaultOption")}
                  </option>
                  {groups
                    .filter((group) => !group.is_default)
                    .map((group) => (
                      <option key={group.id} value={group.id}>
                        {group.name}
                      </option>
                    ))}
                </select>
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
                          onClick={() => void onPickKey()}
                          className="rounded-md border px-3 py-2 text-sm font-medium transition-colors hover:bg-muted"
                        >
                          {t("browse", "Browse")}
                        </button>
                      </div>
                    </div>
                  )}
                </div>
              )}

              <div className="flex items-center justify-between border-b pb-2 pt-2">
                <div className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">
                  {t("sshTunnelPortSettings", "Port Configuration")}
                </div>
                <CommonPortSelectMenu
                  ports={commonPorts}
                  onSelect={onSelectCommonPort}
                  onManage={onManageCommonPorts}
                />
              </div>

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
                      {t("sshTunnelRemoteBindHost", "Remote Listen Host")}
                    </label>
                    <input
                      type="text"
                      value={form.remote_bind_host}
                      onChange={(event) =>
                        setForm((prev) => ({
                          ...prev,
                          remote_bind_host: event.target.value,
                        }))
                      }
                      placeholder={t(
                        "sshTunnelRemoteBindHostPlaceholder",
                        "127.0.0.1, 10.1.3.2, or 0.0.0.0",
                      )}
                      className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
                    />
                  </div>
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
                      "Remote forwarding listens on the host and port you set on the SSH server, then forwards traffic to the service on this device.",
                    )}
                  </div>
                  <div className="rounded-xl border bg-amber-500/10 p-4 text-sm text-amber-700 dark:text-amber-300 md:col-span-2">
                    {t(
                      "sshTunnelRemoteBindScopeHint",
                      "Use 127.0.0.1 to keep the remote port private to the SSH server itself. Use the server IP or 0.0.0.0 only if you want other machines to reach it, and make sure sshd allows GatewayPorts yes or GatewayPorts clientspecified.",
                    )}
                  </div>
                  <div className="space-y-2">
                    <label className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
                      {t("sshTunnelLocalTargetHost", "This Device Service Host")}
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
                      {t("sshTunnelLocalTargetPort", "This Device Service Port")}
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
                <div className="space-y-1 text-sm text-muted-foreground">
                  <div className="font-medium text-foreground">
                    {t("sshTunnelAutoConnectLabel", "Launch at startup")}
                  </div>
                  <div>
                    {t(
                      "sshTunnelAutoConnect",
                      "Automatically connect this tunnel when OneSpace starts.",
                    )}
                  </div>
                </div>
                <Switch
                  checked={form.auto_connect}
                  onCheckedChange={(checked) =>
                    setForm((prev) => ({
                      ...prev,
                      auto_connect: checked,
                    }))
                  }
                  aria-label={t("sshTunnelAutoConnectLabel", "Launch at startup")}
                />
              </div>

              <div className="flex items-center justify-between gap-4 rounded-xl border bg-muted/30 px-4 py-3">
                <div className="space-y-1 text-sm text-muted-foreground">
                  <div className="font-medium text-foreground">
                    {t("sshTunnelAutoReconnectLabel", "Auto reconnect")}
                  </div>
                  <div>
                    {t(
                      "sshTunnelAutoReconnectDesc",
                      "When the SSH connection drops, the network recovers, or the system wakes from sleep, OneSpace will try to restore this tunnel with delay, backoff, and debounce to avoid frequent retries.",
                    )}
                  </div>
                </div>
                <Switch
                  checked={form.auto_reconnect}
                  onCheckedChange={(checked) =>
                    setForm((prev) => ({
                      ...prev,
                      auto_reconnect: checked,
                    }))
                  }
                  aria-label={t("sshTunnelAutoReconnectLabel", "Auto reconnect")}
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
              onClick={onReset}
              className="rounded-md border px-4 py-2 text-sm font-medium transition-colors hover:bg-muted"
            >
              {t("cancel", "Cancel")}
            </button>
            <button
              type="button"
              onClick={onDraftProbe}
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
              onClick={onSave}
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
  );
}
