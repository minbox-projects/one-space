import { CloudCog, Rocket, Server, Settings } from "lucide-react";
import type { TFunction } from "i18next";
import { CustomSelect } from "@/components/common/CustomSelect";
import {
  formatInstalledAt,
  type CloudAgentInstallationInfo,
} from "@/lib/cloudAgent";
import type { CloudAgentHostOption } from "../hooks/useCloudAgent";

interface CloudAgentHeaderProps {
  t: TFunction;
  selectedHost: string;
  setSelectedHost: (host: string) => void;
  hosts?: CloudAgentHostOption[];
  installation?: CloudAgentInstallationInfo | null;
  installationLoading?: boolean;
  language?: string;
  onDeploy?: () => void;
}

export function CloudAgentHeader({
  t,
  selectedHost,
  setSelectedHost,
  hosts = [],
  installation,
  installationLoading = false,
  language,
  onDeploy,
}: CloudAgentHeaderProps) {
  const options =
    hosts.length > 0
      ? hosts.map((host) => ({ value: host.id, label: host.label }))
      : selectedHost
      ? [{ value: selectedHost, label: selectedHost }]
      : [];
  const unknownLabel = t("installationUnknown", "Unknown");

  return (
    <header className="flex flex-wrap items-center justify-between gap-4">
      {/* 左侧：图标 + 标题与简要描述（对齐 AiGateway 标准） */}
      <div className="flex items-start gap-3">
        <div className="rounded-lg p-2 bg-primary/10 text-primary">
          <CloudCog className="h-5 w-5" />
        </div>
        <div className="space-y-0.5">
          <h1 className="text-xl font-bold tracking-tight text-foreground">
            {t("cloudAgentTitle", "AI 云端智能体 (Cloud Agent)")}
          </h1>
          <p className="max-w-2xl text-xs text-muted-foreground">
            {t(
              "cloudAgentSubtitle",
              "长程编程断网保活 · 本地终端一键直连 · AI 网关双端同步 · Issues 自动实施流水线"
            )}
          </p>
        </div>
      </div>

      {/* 右侧：环境主机切换、安装摘要、部署与设置快捷入口 */}
      <div className="flex flex-wrap items-center justify-end gap-2.5 shrink-0">
        {installationLoading ? (
          <span className="text-[11px] text-muted-foreground">
            {t("deploymentStateChecking", "Checking remote installation...")}
          </span>
        ) : installation ? (
          <div
            data-testid="cloud-agent-installation-summary"
            className="flex flex-wrap items-center gap-x-2.5 gap-y-1 rounded-lg border border-border/80 bg-muted/30 px-2.5 py-1.5 text-[11px] text-muted-foreground"
          >
            <span>
              {t("installationRemoteVersion", "Remote version")}{" "}
              <span className="font-mono text-foreground">
                {installation.remoteVersion || unknownLabel}
              </span>
            </span>
            <span className="text-muted-foreground/30">·</span>
            <span>
              {t("installationLocalVersion", "Local latest")}{" "}
              <span className="font-mono text-foreground">
                {installation.localLatestVersion || unknownLabel}
              </span>
            </span>
            <span className="text-muted-foreground/30">·</span>
            <span>
              {t("installationInstalledAt", "Installed at")}{" "}
              <span className="text-foreground">
                {formatInstalledAt(installation.installedAt, language)}
              </span>
            </span>
          </div>
        ) : null}

        <div className="flex items-center gap-1.5 text-xs text-muted-foreground">
          <Server className="h-3.5 w-3.5" />
          <div className="w-36">
            <CustomSelect
              size="sm"
              value={selectedHost}
              onChange={setSelectedHost}
              placeholder={t("cloudAgentSelectHostPlaceholder", "请选择主机")}
              ariaLabel={t("cloudAgentSelectHostPlaceholder", "请选择主机")}
              selectTestId="cloud-agent-host-select"
              options={options}
            />
          </div>
        </div>

        <button
          type="button"
          onClick={onDeploy}
          data-testid="cloud-agent-deploy-open"
          title={t(
            "deployServerHint",
            "Inspect a remote SSH host and deploy the bundled Cloud Agent."
          )}
          className="inline-flex h-8 items-center gap-1.5 rounded-lg border border-border/80 bg-background px-2.5 text-xs font-medium text-foreground shadow-2xs transition-all hover:bg-muted active:scale-98 cursor-pointer"
        >
          <Rocket className="h-3.5 w-3.5 text-primary" />
          <span>{t("deployServer", "Deploy Server")}</span>
        </button>

        <button
          type="button"
          onClick={() => {
            const win = window as any;
            win.setSettingsTab?.("cloud-agent");
            win.setSettingsOpen?.(true);
          }}
          title={t("settings", "设置")}
          aria-label={t("settings", "设置")}
          className="flex h-8 w-8 items-center justify-center rounded-lg border border-border/80 bg-background text-muted-foreground hover:bg-muted hover:text-foreground transition-all cursor-pointer shadow-2xs"
        >
          <Settings className="h-4 w-4" />
        </button>
      </div>
    </header>
  );
}
