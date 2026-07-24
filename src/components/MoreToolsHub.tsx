import { useMemo, useCallback, useState } from "react";
import { Braces, Cloud, Eye, EyeOff, KeyRound, Route, Server, Star, Waypoints } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Bookmarks } from "./Bookmarks";
import { CloudDrive } from "./CloudDrive";
import { SshServers } from "./SshServers";
import { SshTunnels } from "./SshTunnels";
import { ProtocolRouterTool } from "./ProtocolRouterTool";
import { RandomPasswordTool } from "./RandomPasswordTool";
import { JsonParserTool } from "./JsonParserTool";
import type { MoreToolsSection } from "@/lib/navigation";
import {
  readLauncherToolVisibility,
  setLauncherToolVisible,
  type LauncherToolId,
  type LauncherToolVisibility,
} from "@/lib/launcherToolVisibility";

type MoreToolsHubProps = {
  activeTool: MoreToolsSection;
  onSelectTool: (tool: MoreToolsSection) => void;
};

export function MoreToolsHub({
  activeTool,
  onSelectTool,
}: MoreToolsHubProps) {
  const { i18n, t } = useTranslation();
  const [visibility, setVisibility] = useState<LauncherToolVisibility>(() =>
    readLauncherToolVisibility(),
  );

  const handleToggleVisibility = useCallback(
    (toolId: LauncherToolId) => {
      const next = !visibility[toolId];
      setLauncherToolVisible(toolId, next);
      setVisibility((prev) => ({ ...prev, [toolId]: next }));
    },
    [visibility],
  );

  const moreToolsLabel =
    i18n.language === "zh" ? "更多工具" : "More Tools";

  const tools = useMemo(
    () => [
      {
        id: "bookmarks" as const,
        label: i18n.language === "zh" ? "书签" : "Bookmarks",
        description:
          i18n.language === "zh"
            ? "沉淀常用链接和资源入口"
            : "Save the links and resources you revisit often",
        icon: Star,
        launcherToolId: null as LauncherToolId | null,
      },
      {
        id: "cloud" as const,
        label: i18n.language === "zh" ? "云盘" : "Cloud Drive",
        description:
          i18n.language === "zh"
            ? "查看和整理云端文件内容"
            : "Browse and organize synced cloud files",
        icon: Cloud,
        launcherToolId: null as LauncherToolId | null,
      },
      {
        id: "ssh" as const,
        label: i18n.language === "zh" ? "SSH 服务器" : "SSH Servers",
        description:
          i18n.language === "zh"
            ? "集中管理 SSH 配置，快速连接远程主机"
            : "Open saved SSH hosts, history, and custom connections quickly",
        icon: Server,
        launcherToolId: "ssh" as LauncherToolId,
      },
      {
        id: "ssh-tunnels" as const,
        label: i18n.language === "zh" ? "SSH 隧道" : "SSH Tunnels",
        description:
          i18n.language === "zh"
            ? "管理本地、远程和 SOCKS5 动态 SSH 隧道"
            : "Manage local, remote, and dynamic SOCKS5 SSH tunnels",
        icon: Waypoints,
        launcherToolId: "ssh-tunnels" as LauncherToolId,
      },
      {
        id: "protocol-router" as const,
        label: i18n.language === "zh" ? "协议路由" : "Protocol Router",
        description:
          i18n.language === "zh"
            ? "为 Claude Profile 和 OpenAI 兼容供应商暴露本地路由"
            : "Expose local Anthropic-compatible routes for Claude profiles",
        icon: Route,
        launcherToolId: "protocol-router" as LauncherToolId,
      },
      {
        id: "random-password" as const,
        label: t("randomPassword", "Random Password"),
        description: t("randomPasswordToolDesc", "Generate passwords locally with the character groups you need."),
        icon: KeyRound,
        launcherToolId: null as LauncherToolId | null,
      },
      {
        id: "json-parser" as const,
        label: t("jsonParser", "JSON Parser"),
        description: t("jsonParserToolDesc", "Validate and format JSON locally in one editable workspace."),
        icon: Braces,
        launcherToolId: null as LauncherToolId | null,
      },
    ],
    [i18n.language, t],
  );

  const showInLauncherLabel =
    i18n.language === "zh" ? "在启动台展示" : "Show in Launcher";
  const hideInLauncherLabel =
    i18n.language === "zh" ? "不在启动台展示" : "Hide from Launcher";

  return (
    <div className="flex h-full flex-col gap-5">
      <div>
        <h3 className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">
          {moreToolsLabel}
        </h3>
        <p className="mt-1 text-sm text-muted-foreground">
          {i18n.language === "zh"
            ? "把仍然低频的辅助工具收在一起，保持左侧工具分组更聚焦。"
            : "Keep the lower-frequency support tools here so the sidebar stays focused."}
        </p>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-4">
        {tools.map((tool) => {
          const Icon = tool.icon;
          const selected = tool.id === activeTool;
          const isLauncherVisible =
            tool.launcherToolId !== null
              ? visibility[tool.launcherToolId]
              : false;
          const hasToggle = tool.launcherToolId !== null;

          return (
            <button
              key={tool.id}
              type="button"
              onClick={() => onSelectTool(tool.id)}
              className={`group flex min-h-36 flex-col justify-between rounded-xl border p-4 text-left shadow-sm transition-all hover:border-primary/50 hover:shadow-md ${
                selected
                  ? "border-primary bg-primary/5"
                  : "border-transparent bg-card"
              }`}
            >
              <div className="flex items-start justify-between gap-3">
                <div
                  className={`rounded-lg p-2 ${
                    selected
                      ? "bg-primary/10 text-primary"
                      : "bg-emerald-500/10 text-emerald-500"
                  }`}
                >
                  <Icon className="h-6 w-6" />
                </div>
                <div className="flex flex-col items-end gap-2">
                  {hasToggle && (
                    <button
                      type="button"
                      onClick={(e) => {
                        e.stopPropagation();
                        handleToggleVisibility(tool.launcherToolId!);
                      }}
                      className={`inline-flex items-center gap-1 rounded-full border px-2 py-0.5 text-[10px] font-medium transition-colors ${
                        isLauncherVisible
                          ? "border-primary/20 bg-primary/10 text-primary"
                          : "border-muted-foreground/20 bg-muted text-muted-foreground"
                      }`}
                      title={
                        isLauncherVisible
                          ? hideInLauncherLabel
                          : showInLauncherLabel
                      }
                    >
                      {isLauncherVisible ? (
                        <Eye className="h-3 w-3" />
                      ) : (
                        <EyeOff className="h-3 w-3" />
                      )}
                      {i18n.language === "zh" ? "启动台" : "Launcher"}
                    </button>
                  )}
                  <span className="rounded-full border bg-muted px-2 py-0.5 text-[10px] uppercase tracking-[0.16em] text-muted-foreground">
                    {i18n.language === "zh" ? "辅助工具" : "Utility"}
                  </span>
                </div>
              </div>
              <div className="space-y-1">
                <div className="font-semibold">{tool.label}</div>
                <p className="text-sm leading-6 text-muted-foreground">
                  {tool.description}
                </p>
              </div>
            </button>
          );
        })}
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto">
        {activeTool === "bookmarks" ? <Bookmarks /> : null}
        {activeTool === "cloud" ? <CloudDrive /> : null}
        {activeTool === "ssh" ? <SshServers /> : null}
        {activeTool === "ssh-tunnels" ? (
          <SshTunnels isVisible={activeTool === "ssh-tunnels"} />
        ) : null}
        {activeTool === "protocol-router" ? (
          <ProtocolRouterTool isVisible={activeTool === "protocol-router"} />
        ) : null}
        {activeTool === "random-password" ? <RandomPasswordTool /> : null}
        {activeTool === "json-parser" ? <JsonParserTool /> : null}
      </div>
    </div>
  );
}
