import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";

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
  validateServerMonitorDraft,
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
};

function initialState(target: ServerMonitorTargetView | null): FormState {
  return {
    name: target?.name ?? "",
    enabled: target?.enabled ?? true,
    sourceKind: target?.sourceKind ?? "custom",
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
  return {
    id: target?.id ?? null,
    name: form.name.trim(),
    enabled: form.enabled,
    sourceKind: form.sourceKind,
    savedHostName: form.sourceKind === "savedHost" ? form.savedHostName.trim() : null,
    custom,
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

  useEffect(() => {
    if (isOpen) {
      setForm(initialState(target));
      setError(null);
      setTestResult(null);
      setTesting(false);
      setSaving(false);
    }
  }, [isOpen, target]);

  const isEditing = Boolean(target);
  const draft = useMemo(() => buildDraft(form, target), [form, target]);

  const update = <K extends keyof FormState>(key: K, value: FormState[K]) => {
    setForm((previous) => ({ ...previous, [key]: value }));
    setTestResult(null);
  };

  const validationErrorKey = (): string => {
    const name = form.name.trim();
    if (name.length < 1 || name.length > 80) return "serverMonitorValidationName";
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
            <span className="text-sm font-medium">{label("serverMonitorName")}</span>
            <input
              value={form.name}
              maxLength={80}
              onChange={(event) => update("name", event.target.value)}
              className="w-full rounded-lg border border-input bg-background px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-primary/40"
            />
          </label>

          <div className="flex items-center justify-between gap-4">
            <span className="text-sm font-medium">{label("serverMonitorEnabled")}</span>
            <button
              type="button"
              role="switch"
              aria-checked={form.enabled}
              aria-label={`${t("serverMonitorEnabled")} ${alt("serverMonitorEnabled")}`}
              onClick={() => update("enabled", !form.enabled)}
              className={`h-6 w-11 rounded-full border border-transparent transition-colors ${
                form.enabled ? "bg-primary" : "bg-muted"
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
            <legend className="text-sm font-medium">{label("serverMonitorSourceKind")}</legend>
            <div className="flex flex-wrap gap-2">
              {(
                [
                  ["savedHost", "serverMonitorSourceSaved"],
                  ["custom", "serverMonitorSourceCustom"],
                ] as const
              ).map(([value, key]) => (
                <label
                  key={value}
                  className={`inline-flex cursor-pointer items-center gap-2 rounded-lg border px-3 py-1.5 text-xs font-medium ${
                    form.sourceKind === value
                      ? "border-primary bg-primary/10 text-primary"
                      : "border-border bg-background text-muted-foreground"
                  }`}
                >
                  <input
                    type="radio"
                    name="server-monitor-source"
                    value={value}
                    checked={form.sourceKind === value}
                    onChange={() => update("sourceKind", value)}
                    className="sr-only"
                  />
                  {label(key)}
                </label>
              ))}
            </div>
          </fieldset>

          {form.sourceKind === "savedHost" ? (
            <label className="block space-y-1.5">
              <span className="text-sm font-medium">{label("serverMonitorSavedHostName")}</span>
              <input
                value={form.savedHostName}
                onChange={(event) => update("savedHostName", event.target.value)}
                className="w-full rounded-lg border border-input bg-background px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-primary/40"
              />
            </label>
          ) : (
            <div className="space-y-3">
              <div className="grid grid-cols-1 gap-3 sm:grid-cols-[1fr_120px]">
                <label className="block space-y-1.5">
                  <span className="text-sm font-medium">{label("serverMonitorHost")}</span>
                  <input
                    value={form.host}
                    onChange={(event) => update("host", event.target.value)}
                    className="w-full rounded-lg border border-input bg-background px-3 py-2 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-primary/40"
                  />
                </label>
                <label className="block space-y-1.5">
                  <span className="text-sm font-medium">{label("serverMonitorPort")}</span>
                  <input
                    type="number"
                    min={1}
                    max={65535}
                    value={form.port}
                    onChange={(event) => update("port", event.target.value)}
                    className="w-full rounded-lg border border-input bg-background px-3 py-2 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-primary/40"
                  />
                </label>
              </div>
              <label className="block space-y-1.5">
                <span className="text-sm font-medium">{label("serverMonitorUser")}</span>
                <input
                  value={form.user}
                  onChange={(event) => update("user", event.target.value)}
                  className="w-full rounded-lg border border-input bg-background px-3 py-2 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-primary/40"
                />
              </label>
              <div className="space-y-1.5">
                <span className="text-sm font-medium">{label("serverMonitorAuthKind")}</span>
                <CustomSelect
                  ariaLabel={`${t("serverMonitorAuthKind")} ${alt("serverMonitorAuthKind")}`}
                  value={form.authKind}
                  onChange={(value) => update("authKind", value)}
                  options={authOptions}
                />
              </div>
              {form.authKind === "key" ? (
                <label className="block space-y-1.5">
                  <span className="text-sm font-medium">{label("serverMonitorKeyPath")}</span>
                  <input
                    value={form.keyPath}
                    onChange={(event) => update("keyPath", event.target.value)}
                    className="w-full rounded-lg border border-input bg-background px-3 py-2 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-primary/40"
                  />
                </label>
              ) : null}
              {form.authKind === "password" ? (
                <label className="block space-y-1.5">
                  <span className="text-sm font-medium">{label("serverMonitorPassword")}</span>
                  <input
                    type="password"
                    value={form.password}
                    onChange={(event) => update("password", event.target.value)}
                    className="w-full rounded-lg border border-input bg-background px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-primary/40"
                  />
                  {isEditing && target?.hasPassword ? (
                    <span className="block text-xs text-muted-foreground">
                      {label("serverMonitorPasswordKeep")}
                    </span>
                  ) : null}
                </label>
              ) : null}
            </div>
          )}

          {error ? (
            <p role="alert" className="text-sm text-destructive">
              {error}
            </p>
          ) : null}

          {testResult ? (
            <p
              className={`rounded-lg border p-2 text-xs ${
                testResult.succeeded
                  ? "border-emerald-500/30 bg-emerald-500/10 text-emerald-600"
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
          <Button type="button" variant="outline" onClick={onClose} disabled={saving}>
            {label("cancel")}
          </Button>
          <Button
            type="button"
            variant="outline"
            onClick={() => void handleTest()}
            disabled={testing || saving}
          >
            {testing ? label("serverMonitorTesting") : label("serverMonitorTestConnection")}
          </Button>
          <Button type="button" onClick={() => void handleSave()} disabled={saving || testing}>
            {saving ? label("serverMonitorSaving") : label("serverMonitorSave")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
