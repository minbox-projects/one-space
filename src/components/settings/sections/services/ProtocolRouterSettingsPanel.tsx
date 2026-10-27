import type { Dispatch, SetStateAction } from "react";
import type { TFunction } from "i18next";
import { Route, Copy, Key } from "lucide-react";
import { SettingCard } from "../../common/SettingCard";
import type { ProtocolRouterConfig, ProtocolRouterStatus } from "@/lib/protocolRouter";

interface ProtocolRouterSettingsPanelProps {
  t: TFunction;
  protocolRouterConfig: ProtocolRouterConfig;
  setProtocolRouterConfig: Dispatch<SetStateAction<ProtocolRouterConfig>>;
  protocolRouterStatusState: ProtocolRouterStatus | null;
  protocolRouterBusy: boolean;
  onRotateProtocolToken?: () => Promise<void>;
  onShowToast?: (type: "success" | "error", text: string) => void;
}

export function ProtocolRouterSettingsPanel({
  t,
  protocolRouterConfig,
  setProtocolRouterConfig,
  protocolRouterStatusState,
  protocolRouterBusy,
  onRotateProtocolToken,
  onShowToast,
}: ProtocolRouterSettingsPanelProps) {
  return (
    <SettingCard
      id="section-protocol-router"
      icon={Route}
      title={t("protocolRouterSettings", "Protocol Router Settings")}
      description={t(
        "protocolRouterSettingsDesc",
        "Configure the local protocol router port, request retention, and router token. Route status, traffic details, and runtime control now live in the Protocol Router workspace.",
      )}
    >
      {/* Status grid */}
      <div className="grid gap-3 grid-cols-3">
        <div className="rounded-xl border bg-muted/20 p-3.5">
          <div className="text-xs uppercase tracking-wider text-muted-foreground font-semibold">
            {t("status", "Status")}
          </div>
          <div className="mt-1 text-base font-semibold">
            {protocolRouterStatusState?.running
              ? t("running", "Running")
              : t("stopped", "Stopped")}
          </div>
        </div>
        <div className="rounded-xl border bg-muted/20 p-3.5">
          <div className="text-xs uppercase tracking-wider text-muted-foreground font-semibold">
            {t("port", "Port")}
          </div>
          <div className="mt-1 text-base font-semibold">
            {protocolRouterConfig.port}
          </div>
        </div>
        <div className="rounded-xl border bg-muted/20 p-3.5">
          <div className="text-xs uppercase tracking-wider text-muted-foreground font-semibold">
            {t("routes", "Routes")}
          </div>
          <div className="mt-1 text-base font-semibold">
            {protocolRouterConfig.routes.length}
          </div>
        </div>
      </div>

      <div className="grid gap-4 sm:grid-cols-2">
        <div className="space-y-1.5">
          <label className="text-xs font-medium text-muted-foreground">
            {t("port", "Port")}
          </label>
          <input
            type="number"
            min={1}
            max={65535}
            value={protocolRouterConfig.port}
            onChange={(e) =>
              setProtocolRouterConfig((prev) => ({
                ...prev,
                port: parseInt(e.target.value) || 17687,
              }))
            }
            className="w-full bg-background border rounded-xl px-3.5 py-2 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-primary/20"
          />
        </div>

        <div className="space-y-1.5">
          <label className="text-xs font-medium text-muted-foreground">
            {t("retentionDays", "Request History Retention Days")}
          </label>
          <input
            type="number"
            min={1}
            max={365}
            value={protocolRouterConfig.retention_days}
            onChange={(e) =>
              setProtocolRouterConfig((prev) => ({
                ...prev,
                retention_days: Math.min(
                  365,
                  Math.max(1, parseInt(e.target.value) || 30),
                ),
              }))
            }
            className="w-full bg-background border rounded-xl px-3.5 py-2 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-primary/20"
          />
        </div>
      </div>

      {/* Token Management */}
      <div className="space-y-2 rounded-xl border bg-muted/20 p-4">
        <div className="flex items-start justify-between gap-2">
          <div>
            <div className="text-xs font-semibold text-foreground">
              {t("protocolRouterTokenLabel", "Router Token")}
            </div>
            <p className="text-xs text-muted-foreground mt-0.5">
              {t(
                "protocolRouterTokenDesc",
                "Claude profiles use this token to access the local router.",
              )}
            </p>
          </div>
          <div className="text-xs text-muted-foreground font-mono">
            {protocolRouterConfig.token
              ? t("protocolRouterTokenReady", "Generated")
              : t("protocolRouterTokenNotGenerated", "Not generated")}
          </div>
        </div>

        <div className="flex items-center gap-2 pt-1">
          <input
            readOnly
            value={protocolRouterConfig.token}
            placeholder={t("protocolRouterTokenNotGenerated", "Not generated")}
            className="flex-1 bg-background border rounded-xl px-3.5 py-2 font-mono text-xs cursor-default"
          />
          <button
            type="button"
            onClick={async () => {
              try {
                await navigator.clipboard.writeText(
                  protocolRouterConfig.token,
                );
                onShowToast?.("success", t("copiedToClipboard", "Copied to clipboard"));
              } catch (e: any) {
                onShowToast?.("error", "Copy failed");
              }
            }}
            disabled={!protocolRouterConfig.token}
            className="px-3 py-2 rounded-xl border bg-background hover:bg-muted text-xs font-medium inline-flex items-center gap-1.5 transition-all disabled:opacity-50"
          >
            <Copy className="h-3.5 w-3.5" />
            <span>{t("copy", "Copy")}</span>
          </button>
          {onRotateProtocolToken && (
            <button
              type="button"
              onClick={() => void onRotateProtocolToken()}
              disabled={protocolRouterBusy}
              className="px-3 py-2 rounded-xl border bg-background hover:bg-muted text-xs font-medium inline-flex items-center gap-1.5 transition-all disabled:opacity-50"
            >
              <Key className="h-3.5 w-3.5" />
              <span>{t("rotateToken", "Rotate Token")}</span>
            </button>
          )}
        </div>
      </div>

      <div className="rounded-xl border bg-muted/10 p-4 text-xs text-muted-foreground flex items-center justify-between gap-4">
        <div>
          <p className="font-semibold text-foreground">
            {t("protocolRouterOpenToolTitle", "Protocol Router workspace")}
          </p>
          <p className="mt-0.5">
            {t(
              "protocolRouterOpenToolDesc",
              "Open workspace to view runtime traffic, health, and request traces.",
            )}
          </p>
        </div>
        <button
          type="button"
          onClick={() => {
            const appWindow = window as unknown as {
              setActiveTab?: (tab: string) => void;
            };
            appWindow.setActiveTab?.("protocol-router");
          }}
          className="px-3 py-1.5 rounded-lg border bg-background hover:bg-muted text-foreground text-xs font-medium inline-flex items-center gap-1.5 shrink-0"
        >
          <Route className="h-3.5 w-3.5" />
          <span>{t("openProtocolRouterTool", "Open Tool")}</span>
        </button>
      </div>
    </SettingCard>
  );
}
