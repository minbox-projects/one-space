import { AlertCircle, Check, Copy, ExternalLink } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import type { EnvironmentStatus } from "@/lib/portTunnel";
import { useCopyToClipboard } from "@/toolbox/useCopyToClipboard";

interface EnvironmentBannerProps {
  status: EnvironmentStatus | null;
}

export function EnvironmentBanner({ status }: EnvironmentBannerProps) {
  const { t } = useTranslation();
  const { copied, copy } = useCopyToClipboard();

  if (!status || status.installed) {
    return null;
  }

  const binaryName = status.mode === "cloudflare" ? "cloudflared" : "frpc";
  const cmd = status.recommended_command;
  const isUrl = cmd.startsWith("http");

  return (
    <div className="rounded-xl border border-amber-500/30 bg-amber-500/5 p-4 text-amber-900 dark:text-amber-200 shadow-sm">
      <div className="flex items-start justify-between gap-3">
        <div className="flex items-start gap-2.5">
          <AlertCircle className="h-5 w-5 shrink-0 text-amber-500 mt-0.5" />
          <div className="space-y-1">
            <h4 className="text-sm font-semibold text-amber-700 dark:text-amber-300">
              {t("portTunnelMissingBinary", "Dependency Missing")}
            </h4>
            <p className="text-xs text-muted-foreground">
              {t("portTunnelMissingBinaryDesc", {
                binary: binaryName,
                defaultValue: `${binaryName} is not found on your system. Please install it to use this tunnel mode.`,
              })}
            </p>
          </div>
        </div>

        <div className="shrink-0 flex items-center gap-2">
          {isUrl ? (
            <Button
              variant="outline"
              size="sm"
              className="h-8 gap-1.5 text-xs"
              onClick={() => window.open(cmd, "_blank")}
            >
              <ExternalLink className="h-3.5 w-3.5" />
              <span>Download</span>
            </Button>
          ) : (
            <Button
              variant="outline"
              size="sm"
              className="h-8 gap-1.5 text-xs font-mono"
              onClick={() => copy(cmd)}
            >
              {copied ? (
                <>
                  <Check className="h-3.5 w-3.5 text-emerald-500" />
                  <span>{t("portTunnelCmdCopied", "Copied")}</span>
                </>
              ) : (
                <>
                  <Copy className="h-3.5 w-3.5" />
                  <span>{cmd}</span>
                </>
              )}
            </Button>
          )}
        </div>
      </div>
    </div>
  );
}
