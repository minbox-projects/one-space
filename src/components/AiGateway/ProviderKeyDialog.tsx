import { useState } from "react";
import { useTranslation } from "react-i18next";
import { AlertCircle, Clock, Eye, EyeOff, KeyRound, Trash2 } from "lucide-react";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Switch } from "@/components/ui/switch";
import type { GatewayProviderKey } from "@/lib/aiGateway";

export interface ProviderKeyDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  keyData: GatewayProviderKey | null;
  isNew: boolean;
  onConfirm: (name: string, value: string, dailyReset: boolean) => void;
  onDelete?: () => void;
}

interface ProviderKeyFormProps {
  keyData: GatewayProviderKey | null;
  isNew: boolean;
  onConfirm: (name: string, value: string, dailyReset: boolean) => void;
  onCancel: () => void;
  onDelete?: () => void;
}

function ProviderKeyForm({
  keyData,
  isNew,
  onConfirm,
  onCancel,
  onDelete,
}: ProviderKeyFormProps) {
  const { t } = useTranslation();
  const [name, setName] = useState(keyData?.name ?? "");
  const [value, setValue] = useState(keyData?.value ?? "");
  const [dailyReset, setDailyReset] = useState(keyData?.daily_reset === true);
  const [showSecret, setShowSecret] = useState(false);
  const [nameError, setNameError] = useState(false);
  const [valueError, setValueError] = useState(false);

  const handleConfirm = () => {
    const trimmedName = name.trim();
    const trimmedValue = value.trim();

    if (!trimmedName) {
      setNameError(true);
      setValueError(false);
      return;
    }

    if (isNew && !trimmedValue) {
      setValueError(true);
      setNameError(false);
      return;
    }

    setNameError(false);
    setValueError(false);
    onConfirm(trimmedName, value, dailyReset);
  };

  return (
    <>
      <div className="space-y-4 py-1">
        {/* 失效/错误诊断提示 */}
        {keyData?.auto_marked && keyData?.reason && keyData.reason.trim() ? (
          <div
            data-testid="ai-gateway-key-dialog-reason"
            className="rounded-lg border border-destructive/30 bg-destructive/10 p-3 text-xs text-destructive space-y-1"
          >
            <div className="flex items-center gap-1.5 font-semibold">
              <AlertCircle className="h-4 w-4 shrink-0" />
              <span>{t("aiGatewayProviderKeyReasonLabel", "Reason")}</span>
            </div>
            <p className="whitespace-pre-wrap break-all leading-relaxed pl-5.5 text-[11px] text-destructive/90">
              {keyData.reason.trim()}
            </p>
          </div>
        ) : null}

        {/* 密钥名称 */}
        <div className="space-y-1.5">
          <label className="text-xs font-medium text-foreground">
            {t("aiGatewayKeyLabel", "Name")}
            <span className="ml-1 text-destructive">*</span>
          </label>
          <input
            data-testid="ai-gateway-key-dialog-name"
            type="text"
            value={name}
            onChange={(e) => {
              setName(e.target.value);
              if (nameError && e.target.value.trim()) {
                setNameError(false);
              }
            }}
            placeholder={t("aiGatewayKeyLabelPlaceholder", "Key name")}
            className="h-9 w-full rounded-lg border border-border bg-background px-3 text-sm text-foreground outline-none transition-all focus:border-primary focus:ring-2 focus:ring-primary/50"
          />
          {nameError ? (
            <p
              data-testid="ai-gateway-key-name-error"
              role="alert"
              className="mt-1 text-xs text-destructive"
            >
              {t(
                "aiGatewayProviderKeyNameRequired",
                "Enter a key name.",
              )}
            </p>
          ) : null}
        </div>

        {/* 密钥值 */}
        <div className="space-y-1.5">
          <label className="text-xs font-medium text-foreground">
            {t("aiGatewayApiKey", "API Key")}
            {isNew ? <span className="ml-1 text-destructive">*</span> : null}
          </label>
          <div className="relative">
            <input
              data-testid="ai-gateway-key-dialog-value"
              type={showSecret ? "text" : "password"}
              value={value}
              onChange={(e) => {
                setValue(e.target.value);
                if (valueError && e.target.value.trim()) {
                  setValueError(false);
                }
              }}
              placeholder={
                isNew
                  ? "sk-..."
                  : t(
                      "aiGatewayKeyLeaveBlankToKeep",
                      "Leave blank to keep stored secret",
                    )
              }
              className="h-9 w-full rounded-lg border border-border bg-background pl-3 pr-9 text-sm text-foreground font-mono outline-none transition-all focus:border-primary focus:ring-2 focus:ring-primary/50"
            />
            <button
              type="button"
              data-testid="ai-gateway-key-dialog-toggle-visibility"
              onClick={() => setShowSecret((v) => !v)}
              aria-label={
                showSecret
                  ? t("aiGatewayHideSecret", "Hide secret")
                  : t("aiGatewayShowSecret", "Show secret")
              }
              className="absolute right-1.5 top-1/2 inline-flex h-6 w-6 -translate-y-1/2 items-center justify-center rounded-md text-muted-foreground transition hover:bg-muted hover:text-foreground"
            >
              {showSecret ? (
                <EyeOff className="h-3.5 w-3.5" />
              ) : (
                <Eye className="h-3.5 w-3.5" />
              )}
            </button>
          </div>
          {valueError ? (
            <p
              data-testid="ai-gateway-key-value-error"
              role="alert"
              className="mt-1 text-xs text-destructive"
            >
              {t(
                "aiGatewayProviderKeyValueRequired",
                "Enter a value for the new key.",
              )}
            </p>
          ) : null}
        </div>

        {/* 日重置设置 */}
        <div className="flex items-center justify-between rounded-lg border border-border/60 bg-muted/20 px-3 py-2.5">
          <div className="space-y-0.5 pr-3">
            <div className="flex items-center gap-1.5 text-xs font-medium text-foreground">
              <Clock className="h-3.5 w-3.5 text-primary" />
              <span>{t("aiGatewayProviderKeyDailyReset", "Daily reset")}</span>
            </div>
            <p className="text-[11px] text-muted-foreground leading-normal">
              {t(
                "aiGatewayProviderKeyDailyResetHint",
                "When this key hits its quota, it becomes available again after the next UTC+8 midnight.",
              )}
            </p>
          </div>
          <Switch
            data-testid="ai-gateway-key-dialog-daily-reset"
            aria-label={t("aiGatewayProviderKeyDailyReset", "Daily reset")}
            checked={dailyReset}
            onCheckedChange={setDailyReset}
            className="shrink-0"
          />
        </div>
      </div>

      <DialogFooter className="w-full !flex-row items-center justify-between sm:!justify-between gap-2 pt-2">
        <div>
          {!isNew && onDelete ? (
            <button
              type="button"
              data-testid="ai-gateway-key-dialog-delete"
              onClick={onDelete}
              className="inline-flex h-8 items-center justify-center gap-1.5 rounded-lg border border-destructive/20 bg-destructive/5 px-3 text-xs font-medium text-destructive transition hover:bg-destructive/15 hover:border-destructive/40 active:scale-95"
            >
              <Trash2 className="h-3.5 w-3.5" />
              <span>{t("delete", "Delete")}</span>
            </button>
          ) : null}
        </div>
        <div className="flex items-center gap-2">
          <button
            type="button"
            data-testid="ai-gateway-key-dialog-cancel"
            onClick={onCancel}
            className="inline-flex h-8 items-center justify-center rounded-lg border border-border/80 bg-background px-3 text-xs font-medium text-foreground transition hover:bg-muted active:scale-95"
          >
            {t("cancel", "Cancel")}
          </button>
          <button
            type="button"
            data-testid="ai-gateway-key-dialog-confirm"
            onClick={handleConfirm}
            className="inline-flex h-8 items-center justify-center rounded-lg bg-primary px-3 text-xs font-medium text-primary-foreground shadow-xs transition hover:bg-primary/90 active:scale-95"
          >
            {t("confirm", "Confirm")}
          </button>
        </div>
      </DialogFooter>
    </>
  );
}

export function ProviderKeyDialog({
  open,
  onOpenChange,
  keyData,
  isNew,
  onConfirm,
  onDelete,
}: ProviderKeyDialogProps) {
  const { t } = useTranslation();
  const title = isNew
    ? t("aiGatewayProviderKeyAddTitle", "Add upstream key")
    : t("aiGatewayProviderKeyEditTitle", "Edit upstream key");

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        data-testid="ai-gateway-key-dialog"
        className="max-w-md space-y-4"
      >
        <DialogHeader>
          <DialogTitle
            data-testid="ai-gateway-key-dialog-title"
            className="flex items-center gap-2.5 text-base font-semibold"
          >
            <div className="h-7 w-7 rounded-lg bg-primary/10 text-primary flex items-center justify-center shrink-0">
              <KeyRound className="h-4 w-4" />
            </div>
            <span>{title}</span>
          </DialogTitle>
          <DialogDescription className="sr-only">
            {title}
          </DialogDescription>
        </DialogHeader>

        {open ? (
          <ProviderKeyForm
            key={keyData ? keyData.id : "new-key-dialog"}
            keyData={keyData}
            isNew={isNew}
            onConfirm={(n, v, dr) => {
              onConfirm(n, v, dr);
              onOpenChange(false);
            }}
            onCancel={() => onOpenChange(false)}
            onDelete={
              onDelete
                ? () => {
                    onDelete();
                    onOpenChange(false);
                  }
                : undefined
            }
          />
        ) : null}
      </DialogContent>
    </Dialog>
  );
}
