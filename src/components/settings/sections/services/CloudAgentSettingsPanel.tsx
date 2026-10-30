import { useState, useEffect } from "react";
import type { Dispatch, SetStateAction } from "react";
import type { TFunction } from "i18next";
import { Shield, Server, HardDrive, Terminal } from "lucide-react";
import { Switch } from "@/components/ui/switch";
import { SettingCard } from "../../common/SettingCard";
import { CustomSelect } from "@/components/common/CustomSelect";
import { sshHostsList } from "@/lib/sshTunnels";
import type { CloudAgentSettings } from "@/lib/cloudAgent";

interface CloudAgentSettingsPanelProps {
  t: TFunction;
  settings: CloudAgentSettings;
  setSettings: Dispatch<SetStateAction<CloudAgentSettings>>;
  onShowToast?: (type: "success" | "error", text: string) => void;
}

export function CloudAgentSettingsPanel({
  t,
  settings,
  setSettings,
}: CloudAgentSettingsPanelProps) {
  const [sshHosts, setSshHosts] = useState<{ id: string; label: string }[]>([]);

  useEffect(() => {
    sshHostsList<{ name: string; host_name: string }[]>()
      .then((hosts) => {
        if (hosts && hosts.length > 0) {
          setSshHosts(
            hosts.map((h) => ({
              id: h.name,
              label: `${h.name} (${h.host_name})`,
            }))
          );
        }
      })
      .catch(() => {});
  }, []);

  return (
    <div className="space-y-6">
      {/* 1. OpenCode 授权策略与直连终端 */}
      <SettingCard
        id="section-cloud-agent-auth"
        icon={Shield}
        title={t("cloudAgentAuthSettingsTitle", "OpenCode 权限与运行策略")}
        description={t(
          "cloudAgentAuthSettingsDesc",
          "配置云端长程编程与会话启动时的默认授权模式、CLI 参数与直连终端。"
        )}
      >
        <div className="flex items-center justify-between">
          <div>
            <h4 className="text-sm font-semibold text-foreground">
              {t("cloudAgentYoloTitle", "完全自动授权模式 (--yolo)")}
            </h4>
            <p className="text-xs text-muted-foreground mt-0.5">
              {t(
                "cloudAgentYoloDesc",
                "开启后，OpenCode 在云端会话中执行命令、读写文件无需人工逐次确认；关闭后将进入交互式提示模式（开箱即用推荐开启）。"
              )}
            </p>
          </div>
          <Switch
            checked={settings.opencodeYoloMode}
            onCheckedChange={(checked) =>
              setSettings((prev) => ({
                ...prev,
                opencodeYoloMode: checked,
              }))
            }
          />
        </div>

        <hr className="border-border/50" />

        {/* 本地直连终端软件选择 */}
        <div className="space-y-2">
          <label className="text-sm font-semibold text-foreground flex items-center gap-1.5">
            <Terminal className="h-4 w-4 text-primary" />
            {t("cloudAgentTerminalTitle", "本地直连终端软件")}
          </label>
          <p className="text-xs text-muted-foreground">
            {t(
              "cloudAgentTerminalDesc",
              "点击「恢复会话」或连入云端开发环境时调起的 macOS 终端应用。"
            )}
          </p>
          <div className="w-full max-w-sm">
            <CustomSelect
              value={settings.terminalApp || "auto"}
              onChange={(val) =>
                setSettings((prev) => ({
                  ...prev,
                  terminalApp: val,
                }))
              }
              options={[
                { value: "auto", label: "自动检测 / 跟随系统偏好 (Auto)" },
                { value: "Terminal", label: "Terminal.app (系统默认)" },
                { value: "iTerm", label: "iTerm2" },
                { value: "Ghostty", label: "Ghostty" },
                { value: "Alacritty", label: "Alacritty" },
              ]}
            />
          </div>
        </div>

        <hr className="border-border/50" />

        <div className="space-y-2">
          <label className="text-sm font-semibold text-foreground block">
            {t("cloudAgentExtraArgsTitle", "OpenCode 额外启动参数")}
          </label>
          <p className="text-xs text-muted-foreground">
            {t(
              "cloudAgentExtraArgsDesc",
              "追加到 opencode 命令后的全局自定义参数（如 --model cloud-coder 或 --temperature 0.2），默认为空。"
            )}
          </p>
          <input
            type="text"
            value={settings.opencodeExtraArgs}
            onChange={(e) =>
              setSettings((prev) => ({
                ...prev,
                opencodeExtraArgs: e.target.value,
              }))
            }
            placeholder="例如: --model cloud-coder"
            className="w-full rounded-xl border bg-background px-4 py-2 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-primary/20"
          />
        </div>
      </SettingCard>

      {/* 2. 云端主机与部署环境 */}
      <SettingCard
        id="section-cloud-agent-env"
        icon={Server}
        title={t("cloudAgentEnvSettingsTitle", "云端主机与工作空间")}
        description={t(
          "cloudAgentEnvSettingsDesc",
          "配置默认连入的云端 SSH 主机及远端工作目录路径。"
        )}
      >
        <div className="space-y-2">
          <label className="text-sm font-semibold text-foreground block">
            {t("cloudAgentDefaultHostTitle", "默认 SSH 主机")}
          </label>
          <p className="text-xs text-muted-foreground">
            {t(
              "cloudAgentDefaultHostDesc",
              "控制台默认连接与同步的云端服务器（从已保存的 SSH 主机列表中选择）。"
            )}
          </p>
          <div className="w-full max-w-sm">
            <CustomSelect
              value={settings.defaultHost}
              onChange={(val) =>
                setSettings((prev) => ({
                  ...prev,
                  defaultHost: val,
                }))
              }
              options={
                sshHosts.length > 0
                  ? sshHosts.map((h) => ({ value: h.id, label: h.label }))
                  : [{ value: settings.defaultHost, label: settings.defaultHost }]
              }
            />
          </div>
        </div>

        <hr className="border-border/50" />

        <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
          <div className="space-y-2">
            <label className="text-sm font-semibold text-foreground block">
              {t("cloudAgentWorkspaceRootTitle", "远端工作空间根目录")}
            </label>
            <input
              type="text"
              value={settings.remoteWorkspaceRoot}
              onChange={(e) =>
                setSettings((prev) => ({
                  ...prev,
                  remoteWorkspaceRoot: e.target.value,
                }))
              }
              placeholder="/root/cloud-agent-workspaces"
              className="w-full rounded-xl border bg-background px-4 py-2 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-primary/20"
            />
          </div>

          <div className="space-y-2">
            <label className="text-sm font-semibold text-foreground block">
              {t("cloudAgentAgentDirTitle", "远端 Agent 安装目录")}
            </label>
            <input
              type="text"
              value={settings.remoteAgentDir}
              onChange={(e) =>
                setSettings((prev) => ({
                  ...prev,
                  remoteAgentDir: e.target.value,
                }))
              }
              placeholder="/root/onespace-cloud-agent"
              className="w-full rounded-xl border bg-background px-4 py-2 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-primary/20"
            />
          </div>
        </div>
      </SettingCard>

      {/* 3. 监控与预警 */}
      <SettingCard
        id="section-cloud-agent-monitor"
        icon={HardDrive}
        title={t("cloudAgentMonitorSettingsTitle", "监控刷新与空间预警")}
        description={t(
          "cloudAgentMonitorSettingsDesc",
          "配置桌面端轮询状态间隔与云端 VPS 磁盘低空间预警。"
        )}
      >
        <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
          <div className="space-y-2">
            <label className="text-sm font-semibold text-foreground block">
              {t("cloudAgentRefreshIntervalTitle", "自动状态刷新间隔 (秒)")}
            </label>
            <input
              type="number"
              min={3}
              max={120}
              value={settings.autoRefreshIntervalSeconds}
              onChange={(e) =>
                setSettings((prev) => ({
                  ...prev,
                  autoRefreshIntervalSeconds: Number(e.target.value) || 10,
                }))
              }
              className="w-full rounded-xl border bg-background px-4 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-primary/20"
            />
          </div>

          <div className="space-y-2">
            <label className="text-sm font-semibold text-foreground block">
              {t("cloudAgentDiskAlertTitle", "磁盘预警阈值 (%)")}
            </label>
            <input
              type="number"
              min={50}
              max={99}
              value={settings.diskAlertThresholdPercent}
              onChange={(e) =>
                setSettings((prev) => ({
                  ...prev,
                  diskAlertThresholdPercent: Number(e.target.value) || 80,
                }))
              }
              className="w-full rounded-xl border bg-background px-4 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-primary/20"
            />
          </div>
        </div>
      </SettingCard>
    </div>
  );
}
