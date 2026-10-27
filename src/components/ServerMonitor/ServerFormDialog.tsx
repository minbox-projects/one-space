import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { CheckCircle2, Loader2, Server, Sliders } from "lucide-react";

import { CustomSelect } from "@/components/common/CustomSelect";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  loadAvailableSshServers,
  validateServerMonitorDraft,
  type AvailableSshServer,
  type ServerMonitorAuthKind,
  type ServerMonitorSourceKind,
  type ServerMonitorTargetDraft,
  type ServerMonitorTargetView,
  type ServerMonitorTestResult,
} from "@/lib/serverMonitor";

type FormState = {
  name: string;
  enabled: boolean;
  sourceKind: ServerMonitorSourceKind;
  savedHostName: string;
  host: string;
  port: string;
  user: string;
  authKind: ServerMonitorAuthKind;
  keyPath: string;
  password: string;
  tunnelEnabled: boolean;
  tunnelSourceKind: ServerMonitorSourceKind;
  tunnelSavedHostName: string;
  tunnelHost: string;
  tunnelPort: string;
  tunnelUser: string;
  tunnelAuthKind: ServerMonitorAuthKind;
  tunnelKeyPath: string;
  tunnelPassword: string;
};

function initialState(target: ServerMonitorTargetView | null): FormState {
  return {
    name: target?.name ?? "",
    enabled: target?.enabled ?? true,
    sourceKind: target?.sourceKind ?? "savedHost",
    savedHostName: target?.savedHostName ?? "",
    host: target?.host ?? "",
    port: target?.port ? String(target.port) : "22",
    user: target?.user ?? "",
    authKind:
      target?.authKind === "key" || target?.authKind === "password"
        ? target.authKind
        : "password",
    keyPath: target?.keyPath ?? "",
    password: "",
    tunnelEnabled: target?.tunnel?.enabled ?? false,
    tunnelSourceKind: target?.tunnel?.sourceKind ?? "savedHost",
    tunnelSavedHostName: target?.tunnel?.savedHostName ?? "",
    tunnelHost: target?.tunnel?.host ?? "",
    tunnelPort: target?.tunnel?.port ? String(target.tunnel.port) : "22",
    tunnelUser: target?.tunnel?.user ?? "",
    tunnelAuthKind:
      target?.tunnel?.authKind === "key" || target?.tunnel?.authKind === "password"
        ? target.tunnel.authKind
        : "password",
    tunnelKeyPath: target?.tunnel?.keyPath ?? "",
    tunnelPassword: "",
  };
}

function buildDraft(form: FormState, target: ServerMonitorTargetView | null): ServerMonitorTargetDraft {
  const custom =
    form.sourceKind === "custom"
      ? {
          host: form.host.trim(),
          port: Number(form.port),
          user: form.user.trim(),
          authKind: form.authKind,
          keyPath: form.authKind === "key" ? form.keyPath.trim() : null,
          password: form.password ? form.password : null,
        }
      : null;
  const tunnel = form.tunnelEnabled
    ? {
        enabled: true,
        sourceKind: form.tunnelSourceKind,
        savedHostName:
          form.tunnelSourceKind === "savedHost" ? form.tunnelSavedHostName.trim() : null,
        host: form.tunnelSourceKind === "custom" ? form.tunnelHost.trim() : null,
        port: form.tunnelSourceKind === "custom" ? Number(form.tunnelPort) || 22 : null,
        user: form.tunnelSourceKind === "custom" ? form.tunnelUser.trim() : null,
        authKind: form.tunnelSourceKind === "custom" ? form.tunnelAuthKind : null,
        keyPath:
          form.tunnelSourceKind === "custom" && form.tunnelAuthKind === "key"
            ? form.tunnelKeyPath.trim()
            : null,
        password:
          form.tunnelSourceKind === "custom" &&
          form.tunnelAuthKind === "password" &&
          form.tunnelPassword
            ? form.tunnelPassword
            : null,
      }
    : null;
  return {
    id: target?.id ?? null,
    name: form.name.trim(),
    enabled: form.enabled,
    sourceKind: form.sourceKind,
    savedHostName: form.sourceKind === "savedHost" ? form.savedHostName.trim() : null,
    custom,
    tunnel,
  };
}

export interface ServerFormDialogProps {
  isOpen: boolean;
  target: ServerMonitorTargetView | null;
  onClose: () => void;
  onSave: (draft: ServerMonitorTargetDraft) => Promise<void>;
  onTestConnection: (draft: ServerMonitorTargetDraft) => Promise<ServerMonitorTestResult>;
}

export function ServerFormDialog({
  isOpen,
  target,
  onClose,
  onSave,
  onTestConnection,
}: ServerFormDialogProps) {
  const { t, i18n } = useTranslation();
  const language = i18n.resolvedLanguage || i18n.language || "zh";
  const alternate = language.startsWith("zh") ? "en" : "zh";
  const alt = (key: string) => i18n.getFixedT(alternate)(key);

  const [form, setForm] = useState<FormState>(() => initialState(target));
  const [error, setError] = useState<string | null>(null);
  const [testResult, setTestResult] = useState<ServerMonitorTestResult | null>(null);
  const [testing, setTesting] = useState(false);
  const [saving, setSaving] = useState(false);
  const [sshServers, setSshServers] = useState<AvailableSshServer[]>([]);
  const [sshLoading, setSshLoading] = useState(false);

  useEffect(() => {
    if (isOpen) {
      setForm(initialState(target));
      setError(null);
      setTestResult(null);
      setTesting(false);
      setSaving(false);

      setSshLoading(true);
      void loadAvailableSshServers()
        .then((servers) => setSshServers(servers))
        .catch(() => setSshServers([]))
        .finally(() => setSshLoading(false));
    }
  }, [isOpen, target]);

  const isEditing = Boolean(target);
  const draft = useMemo(() => buildDraft(form, target), [form, target]);

  const update = <K extends keyof FormState>(key: K, value: FormState[K]) => {
    setForm((previous) => ({ ...previous, [key]: value }));
    setTestResult(null);
  };

  const handleSelectSshServer = (serverId: string) => {
    const selected = sshServers.find((s) => s.id === serverId || s.name === serverId);
    if (!selected) return;

    setForm((prev) => {
      const next = { ...prev };
      // 自动回填名称（若当前名称为空，或当前名称等于旧 alias/host）
      if (
        !next.name.trim() ||
        next.name === prev.savedHostName ||
        next.name === prev.host
      ) {
        next.name = selected.name;
      }

      if (selected.sourceType === "config") {
        next.sourceKind = "savedHost";
        next.savedHostName = selected.name;
        next.host = selected.host;
        next.port = String(selected.port || 22);
        next.user = selected.user;
      } else {
        // 来自历史的自定义主机，直接回填参数
        next.host = selected.host;
        next.port = String(selected.port || 22);
        next.user = selected.user;
        if (selected.authType === "key") {
          next.authKind = "key";
          if (selected.authVal) next.keyPath = selected.authVal;
        } else if (selected.authType === "password") {
          next.authKind = "password";
        }
      }
      return next;
    });
    setTestResult(null);
  };

  const currentSelectedServer = useMemo(() => {
    if (form.sourceKind === "savedHost" && form.savedHostName) {
      return (
        sshServers.find((s) => s.name === form.savedHostName) ?? null
      );
    }
    return null;
  }, [form.sourceKind, form.savedHostName, sshServers]);

  const validationErrorKey = (): string => {
    const name = form.name.trim();
    if (name.length < 1 || name.length > 80) return "serverMonitorValidationName";

    if (form.tunnelEnabled) {
      if (form.tunnelSourceKind === "savedHost") {
        if (!form.tunnelSavedHostName.trim()) return "serverMonitorValidationTunnelAlias";
      } else {
        if (!form.tunnelHost.trim()) return "serverMonitorValidationTunnelHost";
        const tPort = Number(form.tunnelPort);
        if (!Number.isInteger(tPort) || tPort < 1 || tPort > 65535) {
          return "serverMonitorValidationTunnelPort";
        }
        if (!form.tunnelUser.trim()) return "serverMonitorValidationTunnelUser";
        if (form.tunnelAuthKind === "key" && !form.tunnelKeyPath.trim()) {
          return "serverMonitorValidationTunnelKey";
        }
        if (
          form.tunnelAuthKind === "password" &&
          !form.tunnelPassword &&
          (!target?.tunnel?.hasPassword || !isEditing)
        ) {
          return "serverMonitorValidationTunnelPassword";
        }
      }
    }

    if (form.sourceKind === "savedHost") {
      if (!form.savedHostName.trim()) return "serverMonitorValidationAlias";
      return "";
    }
    if (!form.host.trim()) return "serverMonitorValidationHost";
    const port = Number(form.port);
    if (!Number.isInteger(port) || port < 1 || port > 65535) {
      return "serverMonitorValidationPort";
    }
    if (!form.user.trim()) return "serverMonitorValidationUser";
    if (form.authKind === "key" && !form.keyPath.trim()) {
      return "serverMonitorValidationKey";
    }
    if (form.authKind === "password" && !form.password && !isEditing) {
      return "serverMonitorValidationPassword";
    }
    return "";
  };

  const requireValidDraft = (): boolean => {
    const key = validationErrorKey();
    if (key) {
      setError(`${t(key)} ${alt(key)}`);
      return false;
    }
    if (!validateServerMonitorDraft(draft, target?.id)) {
      setError(`${t("serverMonitorSaveFailed")} ${alt("serverMonitorSaveFailed")}`);
      return false;
    }
    setError(null);
    return true;
  };

  const handleTest = async () => {
    if (!requireValidDraft()) return;
    setTesting(true);
    try {
      const result = await onTestConnection(draft);
      setTestResult(result);
    } catch (reason) {
      setTestResult({
        succeeded: false,
        partial: false,
        errorKind: null,
        error: reason instanceof Error ? reason.message : String(reason),
        snapshot: null,
      });
    } finally {
      setTesting(false);
    }
  };

  const handleSave = async () => {
    if (!requireValidDraft()) return;
    setSaving(true);
    try {
      await onSave(draft);
      onClose();
    } catch (reason) {
      setError(
        reason instanceof Error
          ? reason.message
          : `${t("serverMonitorSaveFailed")} ${alt("serverMonitorSaveFailed")}`,
      );
    } finally {
      setSaving(false);
    }
  };

  const authOptions: { value: ServerMonitorAuthKind; label: string }[] = [
    { value: "password", label: `${t("serverMonitorAuthPassword")} / ${alt("serverMonitorAuthPassword")}` },
    { value: "key", label: `${t("serverMonitorAuthKey")} / ${alt("serverMonitorAuthKey")}` },
  ];

  const label = (key: string) => (
    <>
      {t(key)}
      <span className="sr-only"> {alt(key)}</span>
    </>
  );

  return (
    <Dialog open={isOpen} onOpenChange={(open) => (!open ? onClose() : undefined)}>
      <DialogContent className="max-h-[90vh] overflow-y-auto">
        <DialogHeader>
          <DialogTitle>
            {isEditing ? label("serverMonitorDialogEdit") : label("serverMonitorDialogAdd")}
          </DialogTitle>
          <DialogDescription>{label("serverMonitorDesc")}</DialogDescription>
        </DialogHeader>

        <div className="space-y-4">
          <label className="block space-y-1.5">
            <span className="text-xs font-medium text-slate-700 dark:text-slate-300">
              {label("serverMonitorName")}
            </span>
            <input
              data-testid="target-name-input"
              value={form.name}
              maxLength={80}
              onChange={(event) => update("name", event.target.value)}
              className="w-full rounded-xl border border-slate-200 bg-white px-3.5 py-2 text-sm text-slate-800 shadow-sm focus:border-emerald-500 focus:outline-none focus:ring-1 focus:ring-emerald-500 dark:border-slate-800 dark:bg-slate-900 dark:text-slate-200"
            />
          </label>

          <div className="flex items-center justify-between gap-4 rounded-xl border border-slate-200/80 bg-slate-50/50 px-3.5 py-2.5 dark:border-slate-800/80 dark:bg-slate-900/40">
            <span className="text-xs font-medium text-slate-700 dark:text-slate-300">
              {label("serverMonitorEnabled")}
            </span>
            <button
              type="button"
              role="switch"
              aria-checked={form.enabled}
              aria-label={`${t("serverMonitorEnabled")} ${alt("serverMonitorEnabled")}`}
              onClick={() => update("enabled", !form.enabled)}
              className={`h-6 w-11 rounded-full border border-transparent transition-colors ${
                form.enabled ? "bg-emerald-600" : "bg-slate-200 dark:bg-slate-700"
              }`}
            >
              <span
                className={`block h-5 w-5 rounded-full bg-white shadow transition-transform ${
                  form.enabled ? "translate-x-5" : "translate-x-0"
                }`}
              />
            </button>
          </div>

          <fieldset className="space-y-2">
            <legend className="text-xs font-semibold uppercase tracking-wider text-slate-500 dark:text-slate-400">
              {label("serverMonitorSourceKind")}
            </legend>
            <div className="grid grid-cols-1 gap-2.5 sm:grid-cols-2">
              <label
                data-testid="source-savedHost"
                className={`group relative flex cursor-pointer items-start gap-3 rounded-2xl border p-3.5 transition-all ${
                  form.sourceKind === "savedHost"
                    ? "border-emerald-500/70 bg-emerald-50/60 shadow-sm ring-2 ring-emerald-500/20 dark:border-emerald-500/50 dark:bg-emerald-950/25"
                    : "border-slate-200/80 bg-white hover:border-slate-300 hover:bg-slate-50/70 dark:border-slate-800 dark:bg-slate-900 dark:hover:border-slate-700"
                }`}
              >
                <input
                  type="radio"
                  name="server-monitor-source"
                  value="savedHost"
                  checked={form.sourceKind === "savedHost"}
                  onChange={() => update("sourceKind", "savedHost")}
                  className="sr-only"
                />
                <div
                  className={`mt-0.5 flex h-8 w-8 shrink-0 items-center justify-center rounded-xl transition-colors ${
                    form.sourceKind === "savedHost"
                      ? "bg-emerald-500/20 text-emerald-600 dark:bg-emerald-500/30 dark:text-emerald-400"
                      : "bg-slate-100 text-slate-500 dark:bg-slate-800 dark:text-slate-400"
                  }`}
                >
                  <Server className="h-4 w-4" />
                </div>
                <div className="min-w-0 flex-1">
                  <div className="flex items-center justify-between">
                    <span
                      className={`text-sm font-semibold ${
                        form.sourceKind === "savedHost"
                          ? "text-slate-900 dark:text-white"
                          : "text-slate-700 dark:text-slate-300"
                      }`}
                    >
                      {label("serverMonitorSourceSaved")}
                    </span>
                    <span
                      className={`flex h-4 w-4 shrink-0 items-center justify-center rounded-full border transition-all ${
                        form.sourceKind === "savedHost"
                          ? "border-emerald-500 bg-emerald-500 text-white"
                          : "border-slate-300 dark:border-slate-700"
                      }`}
                    >
                      {form.sourceKind === "savedHost" && (
                        <span className="h-1.5 w-1.5 rounded-full bg-white" />
                      )}
                    </span>
                  </div>
                  <p className="mt-1 text-[11px] leading-relaxed text-slate-500 dark:text-slate-400">
                    {label("serverMonitorSourceSavedDesc")}
                  </p>
                </div>
              </label>

              <label
                data-testid="source-custom"
                className={`group relative flex cursor-pointer items-start gap-3 rounded-2xl border p-3.5 transition-all ${
                  form.sourceKind === "custom"
                    ? "border-emerald-500/70 bg-emerald-50/60 shadow-sm ring-2 ring-emerald-500/20 dark:border-emerald-500/50 dark:bg-emerald-950/25"
                    : "border-slate-200/80 bg-white hover:border-slate-300 hover:bg-slate-50/70 dark:border-slate-800 dark:bg-slate-900 dark:hover:border-slate-700"
                }`}
              >
                <input
                  type="radio"
                  name="server-monitor-source"
                  value="custom"
                  checked={form.sourceKind === "custom"}
                  onChange={() => update("sourceKind", "custom")}
                  className="sr-only"
                />
                <div
                  className={`mt-0.5 flex h-8 w-8 shrink-0 items-center justify-center rounded-xl transition-colors ${
                    form.sourceKind === "custom"
                      ? "bg-emerald-500/20 text-emerald-600 dark:bg-emerald-500/30 dark:text-emerald-400"
                      : "bg-slate-100 text-slate-500 dark:bg-slate-800 dark:text-slate-400"
                  }`}
                >
                  <Sliders className="h-4 w-4" />
                </div>
                <div className="min-w-0 flex-1">
                  <div className="flex items-center justify-between">
                    <span
                      className={`text-sm font-semibold ${
                        form.sourceKind === "custom"
                          ? "text-slate-900 dark:text-white"
                          : "text-slate-700 dark:text-slate-300"
                      }`}
                    >
                      {label("serverMonitorSourceCustom")}
                    </span>
                    <span
                      className={`flex h-4 w-4 shrink-0 items-center justify-center rounded-full border transition-all ${
                        form.sourceKind === "custom"
                          ? "border-emerald-500 bg-emerald-500 text-white"
                          : "border-slate-300 dark:border-slate-700"
                      }`}
                    >
                      {form.sourceKind === "custom" && (
                        <span className="h-1.5 w-1.5 rounded-full bg-white" />
                      )}
                    </span>
                  </div>
                  <p className="mt-1 text-[11px] leading-relaxed text-slate-500 dark:text-slate-400">
                    {label("serverMonitorSourceCustomDesc")}
                  </p>
                </div>
              </label>
            </div>
          </fieldset>

          {form.sourceKind === "savedHost" ? (
            <div className="space-y-3">
              <div className="space-y-1.5">
                <div className="flex items-center justify-between text-xs font-medium text-slate-700 dark:text-slate-300">
                  <div className="flex items-center gap-1.5">
                    <span>{label("serverMonitorSelectFromSsh")}</span>
                    {sshServers.length > 0 ? (
                      <span className="rounded-full bg-slate-100 px-2 py-0.5 text-[10px] font-mono font-medium text-slate-600 dark:bg-slate-800 dark:text-slate-300">
                        {sshServers.length}
                      </span>
                    ) : null}
                  </div>
                  {sshLoading ? (
                    <span className="flex items-center gap-1 text-[11px] text-slate-400">
                      <Loader2 className="h-3 w-3 animate-spin" />
                      {label("serverMonitorLoading")}
                    </span>
                  ) : null}
                </div>

                {sshServers.length > 0 ? (
                  <select
                    aria-label={`${t("serverMonitorSelectFromSsh")} ${alt("serverMonitorSelectFromSsh")}`}
                    value={
                      sshServers.find((s) => s.name === form.savedHostName)?.id ??
                      (form.savedHostName ? "custom_manual" : "")
                    }
                    onChange={(event) => {
                      const val = event.target.value;
                      if (!val) {
                        update("savedHostName", "");
                        return;
                      }
                      if (val === "custom_manual") return;
                      handleSelectSshServer(val);
                    }}
                    className="w-full rounded-xl border border-slate-200 bg-white px-3.5 py-2 text-sm text-slate-800 shadow-sm focus:border-emerald-500 focus:outline-none focus:ring-1 focus:ring-emerald-500 dark:border-slate-800 dark:bg-slate-900 dark:text-slate-200"
                  >
                    <option value="">{t("serverMonitorSshServerChoose")}</option>
                    {sshServers.map((s) => (
                      <option key={s.id} value={s.id}>
                        {s.name} · {s.user}@{s.host}:{s.port} (
                        {s.sourceType === "config"
                          ? t("serverMonitorSshSourceConfig")
                          : t("serverMonitorSshSourceHistory")}
                        )
                      </option>
                    ))}
                    {form.savedHostName &&
                    !sshServers.some((s) => s.name === form.savedHostName) ? (
                      <option value="custom_manual">{form.savedHostName}</option>
                    ) : null}
                  </select>
                ) : !sshLoading ? (
                  <div className="rounded-2xl border border-dashed border-slate-200 p-4 text-center text-xs dark:border-slate-800">
                    <Server className="mx-auto h-5 w-5 text-slate-400" />
                    <p className="mt-2 font-medium text-slate-800 dark:text-slate-200">
                      {t("serverMonitorNoSshServers")}
                    </p>
                    <p className="mt-1 text-slate-500 dark:text-slate-400">
                      {t("serverMonitorNoSshServersDesc")}
                    </p>
                    <button
                      type="button"
                      onClick={() => update("sourceKind", "custom")}
                      className="mt-3 inline-flex items-center gap-1 rounded-xl border border-slate-200 bg-white px-3 py-1 text-xs font-medium text-slate-700 shadow-sm transition hover:bg-slate-50 dark:border-slate-700 dark:bg-slate-800 dark:text-slate-300"
                    >
                      切换为自定义连接
                    </button>
                  </div>
                ) : null}
              </div>

              {currentSelectedServer ? (
                <div className="rounded-2xl border border-emerald-500/30 bg-emerald-50/50 p-3.5 text-xs dark:border-emerald-500/20 dark:bg-emerald-950/20">
                  <div className="flex items-center justify-between">
                    <div className="flex items-center gap-2">
                      <span className="flex h-2 w-2 rounded-full bg-emerald-500 ring-4 ring-emerald-500/20" />
                      <span className="font-semibold text-slate-900 dark:text-white">
                        {currentSelectedServer.name}
                      </span>
                      <span className="rounded-md bg-emerald-100 px-1.5 py-0.5 text-[10px] font-mono text-emerald-800 dark:bg-emerald-900/40 dark:text-emerald-300">
                        {currentSelectedServer.sourceType === "config"
                          ? t("serverMonitorSshSourceConfig")
                          : t("serverMonitorSshSourceHistory")}
                      </span>
                    </div>
                    <span className="flex items-center gap-1 text-[11px] font-medium text-emerald-600 dark:text-emerald-400">
                      <CheckCircle2 className="h-3.5 w-3.5" />
                      已关联
                    </span>
                  </div>
                  <div className="mt-2 flex flex-wrap items-center gap-x-3 gap-y-1 font-mono text-slate-600 dark:text-slate-400">
                    <span>
                      {currentSelectedServer.user}@{currentSelectedServer.host}:
                      {currentSelectedServer.port}
                    </span>
                    {currentSelectedServer.authType ? (
                      <span className="text-[11px] text-slate-400">
                        ({currentSelectedServer.authType === "key" ? "私钥认证" : "密码认证"})
                      </span>
                    ) : null}
                  </div>
                </div>
              ) : null}

              <label className="block space-y-1.5 pt-1">
                <div className="flex items-center justify-between">
                  <span className="text-xs font-medium text-slate-700 dark:text-slate-300">
                    {label("serverMonitorSavedHostName")}
                  </span>
                  <span className="text-[11px] text-slate-400">
                    (匹配 ~/.ssh/config 的 Host 别名)
                  </span>
                </div>
                <input
                  data-testid="target-saved-host-input"
                  value={form.savedHostName}
                  placeholder="~/.ssh/config Host alias"
                  onChange={(event) => update("savedHostName", event.target.value)}
                  className="w-full rounded-xl border border-slate-200 bg-white px-3.5 py-2 text-sm font-mono text-slate-800 shadow-sm focus:border-emerald-500 focus:outline-none focus:ring-1 focus:ring-emerald-500 dark:border-slate-800 dark:bg-slate-900 dark:text-slate-200"
                />
              </label>
            </div>
          ) : (
            <div className="space-y-3">
              <div className="grid grid-cols-1 gap-3 sm:grid-cols-[1fr_120px]">
                <label className="block space-y-1.5">
                  <span className="text-xs font-medium text-slate-700 dark:text-slate-300">
                    {label("serverMonitorHost")}
                  </span>
                  <input
                    data-testid="target-host-input"
                    value={form.host}
                    onChange={(event) => update("host", event.target.value)}
                    className="w-full rounded-xl border border-slate-200 bg-white px-3.5 py-2 text-sm font-mono text-slate-800 shadow-sm focus:border-emerald-500 focus:outline-none focus:ring-1 focus:ring-emerald-500 dark:border-slate-800 dark:bg-slate-900 dark:text-slate-200"
                  />
                </label>
                <label className="block space-y-1.5">
                  <span className="text-xs font-medium text-slate-700 dark:text-slate-300">
                    {label("serverMonitorPort")}
                  </span>
                  <input
                    data-testid="target-port-input"
                    type="number"
                    min={1}
                    max={65535}
                    value={form.port}
                    onChange={(event) => update("port", event.target.value)}
                    className="w-full rounded-xl border border-slate-200 bg-white px-3.5 py-2 text-sm font-mono text-slate-800 shadow-sm focus:border-emerald-500 focus:outline-none focus:ring-1 focus:ring-emerald-500 dark:border-slate-800 dark:bg-slate-900 dark:text-slate-200"
                  />
                </label>
              </div>
              <label className="block space-y-1.5">
                <span className="text-xs font-medium text-slate-700 dark:text-slate-300">
                  {label("serverMonitorUser")}
                </span>
                <input
                  data-testid="target-user-input"
                  value={form.user}
                  onChange={(event) => update("user", event.target.value)}
                  className="w-full rounded-xl border border-slate-200 bg-white px-3.5 py-2 text-sm font-mono text-slate-800 shadow-sm focus:border-emerald-500 focus:outline-none focus:ring-1 focus:ring-emerald-500 dark:border-slate-800 dark:bg-slate-900 dark:text-slate-200"
                />
              </label>
              <div className="space-y-1.5">
                <span className="text-xs font-medium text-slate-700 dark:text-slate-300">
                  {label("serverMonitorAuthKind")}
                </span>
                <CustomSelect
                  ariaLabel={`${t("serverMonitorAuthKind")} ${alt("serverMonitorAuthKind")}`}
                  value={form.authKind}
                  onChange={(value) => update("authKind", value)}
                  options={authOptions}
                />
              </div>
              {form.authKind === "key" ? (
                <label className="block space-y-1.5">
                  <span className="text-xs font-medium text-slate-700 dark:text-slate-300">
                    {label("serverMonitorKeyPath")}
                  </span>
                  <input
                    value={form.keyPath}
                    onChange={(event) => update("keyPath", event.target.value)}
                    className="w-full rounded-xl border border-slate-200 bg-white px-3.5 py-2 text-sm font-mono text-slate-800 shadow-sm focus:border-emerald-500 focus:outline-none focus:ring-1 focus:ring-emerald-500 dark:border-slate-800 dark:bg-slate-900 dark:text-slate-200"
                  />
                </label>
              ) : null}
              {form.authKind === "password" ? (
                <label className="block space-y-1.5">
                  <span className="text-xs font-medium text-slate-700 dark:text-slate-300">
                    {label("serverMonitorPassword")}
                  </span>
                  <input
                    data-testid="target-password-input"
                    type="password"
                    value={form.password}
                    onChange={(event) => update("password", event.target.value)}
                    className="w-full rounded-xl border border-slate-200 bg-white px-3.5 py-2 text-sm text-slate-800 shadow-sm focus:border-emerald-500 focus:outline-none focus:ring-1 focus:ring-emerald-500 dark:border-slate-800 dark:bg-slate-900 dark:text-slate-200"
                  />
                  {isEditing && target?.hasPassword ? (
                    <span className="block text-xs text-slate-400">
                      {label("serverMonitorPasswordKeep")}
                    </span>
                  ) : null}
                </label>
              ) : null}
            </div>
          )}

          {/* SSH 隧道 / 跳板机配置 (针对无公网 IP 服务器) */}
          <div className="rounded-2xl border border-slate-200/80 bg-slate-50/50 p-4 space-y-3 dark:border-slate-800/80 dark:bg-slate-900/40">
            <div className="flex items-center justify-between gap-4">
              <div className="space-y-0.5">
                <span className="text-xs font-semibold text-slate-800 dark:text-slate-200">
                  {label("serverMonitorTunnelEnableTitle")}
                </span>
                <p className="text-[11px] text-slate-500 dark:text-slate-400">
                  {label("serverMonitorTunnelEnableDesc")}
                </p>
              </div>
              <button
                type="button"
                role="switch"
                aria-checked={form.tunnelEnabled}
                aria-label={`${t("serverMonitorTunnelEnableTitle")} ${alt("serverMonitorTunnelEnableTitle")}`}
                onClick={() => update("tunnelEnabled", !form.tunnelEnabled)}
                className={`h-6 w-11 shrink-0 rounded-full border border-transparent transition-colors ${
                  form.tunnelEnabled ? "bg-emerald-600" : "bg-slate-200 dark:bg-slate-700"
                }`}
              >
                <span
                  className={`block h-5 w-5 rounded-full bg-white shadow transition-transform ${
                    form.tunnelEnabled ? "translate-x-5" : "translate-x-0"
                  }`}
                />
              </button>
            </div>

            {form.tunnelEnabled ? (
              <div className="space-y-3 pt-2 border-t border-slate-200/60 dark:border-slate-800/60">
                <div className="flex gap-4">
                  <label className="flex items-center gap-1.5 text-xs text-slate-700 dark:text-slate-300 cursor-pointer">
                    <input
                      type="radio"
                      name="tunnel-source"
                      value="savedHost"
                      checked={form.tunnelSourceKind === "savedHost"}
                      onChange={() => update("tunnelSourceKind", "savedHost")}
                      className="text-emerald-600 focus:ring-emerald-500"
                    />
                    <span>{label("serverMonitorTunnelSourceSaved")}</span>
                  </label>
                  <label className="flex items-center gap-1.5 text-xs text-slate-700 dark:text-slate-300 cursor-pointer">
                    <input
                      type="radio"
                      name="tunnel-source"
                      value="custom"
                      checked={form.tunnelSourceKind === "custom"}
                      onChange={() => update("tunnelSourceKind", "custom")}
                      className="text-emerald-600 focus:ring-emerald-500"
                    />
                    <span>{label("serverMonitorTunnelSourceCustom")}</span>
                  </label>
                </div>

                {form.tunnelSourceKind === "savedHost" ? (
                  <div className="space-y-2">
                    <label className="block space-y-1.5">
                      <span className="text-xs font-medium text-slate-700 dark:text-slate-300">
                        {label("serverMonitorTunnelSelectHost")}
                      </span>
                      <select
                        data-testid="tunnel-saved-host-select"
                        aria-label={`${t("serverMonitorTunnelSelectHost")} ${alt("serverMonitorTunnelSelectHost")}`}
                        value={
                          sshServers.find((s) => s.name === form.tunnelSavedHostName)?.id ??
                          (form.tunnelSavedHostName ? "custom_tunnel_manual" : "")
                        }
                        onChange={(e) => {
                          const val = e.target.value;
                          if (!val) {
                            update("tunnelSavedHostName", "");
                            return;
                          }
                          const s = sshServers.find((srv) => srv.id === val || srv.name === val);
                          if (s) {
                            update("tunnelSavedHostName", s.name);
                          }
                        }}
                        className="w-full rounded-xl border border-slate-200 bg-white px-3.5 py-2 text-sm text-slate-800 shadow-sm focus:border-emerald-500 focus:outline-none focus:ring-1 focus:ring-emerald-500 dark:border-slate-800 dark:bg-slate-900 dark:text-slate-200"
                      >
                        <option value="">{t("serverMonitorSshServerChoose")}</option>
                        {sshServers.map((s) => (
                          <option key={s.id} value={s.id}>
                            {s.name} · {s.user}@{s.host}:{s.port}
                          </option>
                        ))}
                      </select>
                    </label>
                    <label className="block space-y-1.5">
                      <span className="text-xs font-medium text-slate-700 dark:text-slate-300">
                        {label("serverMonitorSavedHostName")}
                      </span>
                      <input
                        value={form.tunnelSavedHostName}
                        placeholder="~/.ssh/config Host alias"
                        onChange={(e) => update("tunnelSavedHostName", e.target.value)}
                        className="w-full rounded-xl border border-slate-200 bg-white px-3.5 py-2 text-sm font-mono text-slate-800 shadow-sm focus:border-emerald-500 focus:outline-none focus:ring-1 focus:ring-emerald-500 dark:border-slate-800 dark:bg-slate-900 dark:text-slate-200"
                      />
                    </label>
                  </div>
                ) : (
                  <div className="space-y-3">
                    <div className="grid grid-cols-1 gap-3 sm:grid-cols-[1fr_120px]">
                      <label className="block space-y-1.5">
                        <span className="text-xs font-medium text-slate-700 dark:text-slate-300">
                          {label("serverMonitorTunnelHost")}
                        </span>
                        <input
                          value={form.tunnelHost}
                          placeholder="跳板机公网 IP / 域名"
                          onChange={(e) => update("tunnelHost", e.target.value)}
                          className="w-full rounded-xl border border-slate-200 bg-white px-3.5 py-2 text-sm font-mono text-slate-800 shadow-sm focus:border-emerald-500 focus:outline-none focus:ring-1 focus:ring-emerald-500 dark:border-slate-800 dark:bg-slate-900 dark:text-slate-200"
                        />
                      </label>
                      <label className="block space-y-1.5">
                        <span className="text-xs font-medium text-slate-700 dark:text-slate-300">
                          {label("serverMonitorPort")}
                        </span>
                        <input
                          type="number"
                          min={1}
                          max={65535}
                          value={form.tunnelPort}
                          onChange={(e) => update("tunnelPort", e.target.value)}
                          className="w-full rounded-xl border border-slate-200 bg-white px-3.5 py-2 text-sm font-mono text-slate-800 shadow-sm focus:border-emerald-500 focus:outline-none focus:ring-1 focus:ring-emerald-500 dark:border-slate-800 dark:bg-slate-900 dark:text-slate-200"
                        />
                      </label>
                    </div>
                    <label className="block space-y-1.5">
                      <span className="text-xs font-medium text-slate-700 dark:text-slate-300">
                        {label("serverMonitorUser")}
                      </span>
                      <input
                        value={form.tunnelUser}
                        onChange={(e) => update("tunnelUser", e.target.value)}
                        className="w-full rounded-xl border border-slate-200 bg-white px-3.5 py-2 text-sm font-mono text-slate-800 shadow-sm focus:border-emerald-500 focus:outline-none focus:ring-1 focus:ring-emerald-500 dark:border-slate-800 dark:bg-slate-900 dark:text-slate-200"
                      />
                    </label>
                    <div className="space-y-1.5">
                      <span className="text-xs font-medium text-slate-700 dark:text-slate-300">
                        {label("serverMonitorAuthKind")}
                      </span>
                      <CustomSelect
                        ariaLabel={`${t("serverMonitorAuthKind")} ${alt("serverMonitorAuthKind")}`}
                        value={form.tunnelAuthKind}
                        onChange={(value) => update("tunnelAuthKind", value)}
                        options={authOptions}
                      />
                    </div>
                    {form.tunnelAuthKind === "key" ? (
                      <label className="block space-y-1.5">
                        <span className="text-xs font-medium text-slate-700 dark:text-slate-300">
                          {label("serverMonitorKeyPath")}
                        </span>
                        <input
                          value={form.tunnelKeyPath}
                          onChange={(e) => update("tunnelKeyPath", e.target.value)}
                          className="w-full rounded-xl border border-slate-200 bg-white px-3.5 py-2 text-sm font-mono text-slate-800 shadow-sm focus:border-emerald-500 focus:outline-none focus:ring-1 focus:ring-emerald-500 dark:border-slate-800 dark:bg-slate-900 dark:text-slate-200"
                        />
                      </label>
                    ) : (
                      <label className="block space-y-1.5">
                        <span className="text-xs font-medium text-slate-700 dark:text-slate-300">
                          {label("serverMonitorPassword")}
                        </span>
                        <input
                          type="password"
                          value={form.tunnelPassword}
                          onChange={(e) => update("tunnelPassword", e.target.value)}
                          className="w-full rounded-xl border border-slate-200 bg-white px-3.5 py-2 text-sm text-slate-800 shadow-sm focus:border-emerald-500 focus:outline-none focus:ring-1 focus:ring-emerald-500 dark:border-slate-800 dark:bg-slate-900 dark:text-slate-200"
                        />
                        {isEditing && target?.tunnel?.hasPassword ? (
                          <span className="block text-xs text-slate-400">
                            {label("serverMonitorPasswordKeep")}
                          </span>
                        ) : null}
                      </label>
                    )}
                  </div>
                )}
              </div>
            ) : null}
          </div>

          {error ? (
            <p role="alert" className="text-xs text-destructive">
              {error}
            </p>
          ) : null}

          {testResult ? (
            <p
              className={`rounded-xl border p-2.5 text-xs ${
                testResult.succeeded
                  ? "border-emerald-500/30 bg-emerald-50/70 text-emerald-700 dark:border-emerald-500/20 dark:bg-emerald-950/30 dark:text-emerald-300"
                  : "border-destructive/30 bg-destructive/10 text-destructive"
              }`}
            >
              {testResult.succeeded
                ? testResult.partial
                  ? label("serverMonitorTestPartial")
                  : label("serverMonitorTestOk")
                : testResult.error ||
                  `${t("serverMonitorTestFailed")} ${alt("serverMonitorTestFailed")}`}
            </p>
          ) : null}
        </div>

        <DialogFooter className="gap-2">
          <Button
            type="button"
            variant="outline"
            onClick={onClose}
            disabled={saving}
            className="rounded-xl border-slate-200 dark:border-slate-800"
          >
            {label("cancel")}
          </Button>
          <Button
            type="button"
            variant="outline"
            onClick={() => void handleTest()}
            disabled={testing || saving}
            className="rounded-xl border-slate-200 dark:border-slate-800"
          >
            {testing ? label("serverMonitorTesting") : label("serverMonitorTestConnection")}
          </Button>
          <Button
            type="button"
            onClick={() => void handleSave()}
            disabled={saving || testing}
            className="rounded-xl bg-slate-900 text-white shadow-sm hover:bg-slate-800 dark:bg-emerald-600 dark:hover:bg-emerald-700"
          >
            {saving ? label("serverMonitorSaving") : label("serverMonitorSave")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
