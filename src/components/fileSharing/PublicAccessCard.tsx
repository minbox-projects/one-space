import {
  AlertTriangle,
  Check,
  Copy,
  ExternalLink,
  Globe2,
  Loader2,
  Lock,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import type { FileSharingPublicState } from "@/lib/fileSharing";

interface PublicAccessCardProps {
  state: FileSharingPublicState;
  publicUrl: string | null;
  error: string | null;
  passwordProtected: boolean;
  busy: boolean;
  missingDependency: boolean;
  copied: boolean;
  onEnable: () => void;
  onDisable: () => void;
  onRetry: () => void;
  onCopy: () => void;
}

export function PublicAccessCard({
  state,
  publicUrl,
  error,
  passwordProtected,
  busy,
  missingDependency,
  copied,
  onEnable,
  onDisable,
  onRetry,
  onCopy,
}: PublicAccessCardProps) {
  const { t } = useTranslation();

  const stateLabel =
    state === "running"
      ? t("fileSharingPublicRunning", "Public access is active")
      : state === "starting"
        ? t("fileSharingPublicStarting", "Starting public access...")
        : state === "error"
          ? t("fileSharingPublicError", "Public access failed")
          : null;

  return (
    <section
      data-testid="file-sharing-public-card"
      data-state={state}
      className="space-y-3 rounded-md border p-3"
    >
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div className="flex items-center gap-2">
          <Globe2 className="h-4 w-4 text-muted-foreground" />
          <h3 className="text-sm font-semibold">
            {t("fileSharingPublicTitle", "Public access")}
          </h3>
          {passwordProtected ? (
            <span className="inline-flex items-center gap-1 rounded-full border px-2 py-0.5 text-xs text-muted-foreground">
              <Lock className="h-3 w-3" />
              {t("fileSharingPasswordProtected", "Password protected")}
            </span>
          ) : null}
        </div>
        {stateLabel ? (
          <span className="text-xs text-muted-foreground">{stateLabel}</span>
        ) : null}
      </div>

      {state === "starting" ? (
        <p className="flex items-center gap-2 text-sm text-muted-foreground">
          <Loader2 className="h-4 w-4 animate-spin" />
          {t("fileSharingPublicStarting", "Starting public access...")}
        </p>
      ) : null}

      {state === "running" && publicUrl ? (
        <div className="space-y-2">
          <div
            data-testid="file-sharing-public-url"
            className="rounded-md border bg-muted/40 p-3 font-mono text-sm break-all"
          >
            {publicUrl}
          </div>
          <div className="flex flex-wrap gap-2">
            <button
              type="button"
              onClick={onCopy}
              className="inline-flex h-9 items-center gap-2 rounded-md border px-3 text-sm hover:bg-muted"
            >
              {copied ? (
                <Check className="h-4 w-4 text-emerald-600" />
              ) : (
                <Copy className="h-4 w-4" />
              )}
              {t("fileSharingCopyLink", "Copy link")}
            </button>
            <button
              type="button"
              onClick={() => window.open(publicUrl, "_blank", "noopener,noreferrer")}
              className="inline-flex h-9 items-center gap-2 rounded-md border px-3 text-sm hover:bg-muted"
            >
              <ExternalLink className="h-4 w-4" />
              {t("fileSharingOpenLink", "Open link")}
            </button>
          </div>
        </div>
      ) : null}

      {state === "error" ? (
        <div className="flex items-start gap-2 rounded-md border border-destructive/30 bg-destructive/5 p-2 text-sm text-destructive">
          <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" />
          <span className="min-w-0 break-words">
            {error || t("fileSharingPublicError", "Public access failed")}
          </span>
        </div>
      ) : null}

      <div className="flex flex-wrap gap-2">
        <button
          type="button"
          data-testid="file-sharing-public-enable"
          onClick={onEnable}
          disabled={busy || missingDependency}
          className="inline-flex h-9 items-center gap-2 rounded-md border px-3 text-sm hover:bg-muted disabled:cursor-not-allowed disabled:opacity-50"
        >
          <Globe2 className="h-4 w-4" />
          {t("fileSharingPublicEnable", "Enable public access")}
        </button>
        {state === "starting" || state === "running" ? (
          <button
            type="button"
            data-testid="file-sharing-public-disable"
            onClick={onDisable}
            disabled={busy}
            className="inline-flex h-9 items-center gap-2 rounded-md border px-3 text-sm hover:bg-muted disabled:cursor-not-allowed disabled:opacity-50"
          >
            {t("fileSharingPublicDisable", "Disable public access")}
          </button>
        ) : null}
        {state === "error" ? (
          <button
            type="button"
            data-testid="file-sharing-public-retry"
            onClick={onRetry}
            disabled={busy || missingDependency}
            className="inline-flex h-9 items-center gap-2 rounded-md border px-3 text-sm hover:bg-muted disabled:cursor-not-allowed disabled:opacity-50"
          >
            {t("fileSharingPublicRetry", "Retry public access")}
          </button>
        ) : null}
      </div>
    </section>
  );
}
