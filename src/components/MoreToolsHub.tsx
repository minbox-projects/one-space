import { useMemo, useCallback, useState } from "react";
import { ArrowLeft, GripVertical } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Bookmarks } from "./Bookmarks";
import { CloudDrive } from "./CloudDrive";
import { SshServers } from "./SshServers";
import { SshTunnels } from "./SshTunnels";
import { ProtocolRouterTool } from "./ProtocolRouterTool";
import { RandomPasswordTool } from "./RandomPasswordTool";
import { JsonParserTool } from "./JsonParserTool";
import { Md5EncryptionTool } from "./Md5EncryptionTool";
import { ShortLinkTool } from "./ShortLinkTool";
import { FileSharingTool } from "./FileSharingTool";
import { JttDataParserTool } from "./JttDataParserTool";
import { Switch } from "./ui/switch";
import type { JttParserTab, MoreToolsSection } from "@/lib/navigation";
import { getMoreToolPresentation } from "@/lib/moreToolPresentation";
import {
  MORE_TOOLS_ORDER_KEY,
  applySavedOrder,
  readSavedOrder,
  writeSavedOrder,
} from "@/lib/launcherToolOrder";
import { useCardDragReorder } from "@/lib/useCardDragReorder";
import {
  readLauncherToolVisibility,
  setLauncherToolVisible,
  type LauncherToolId,
  type LauncherToolVisibility,
} from "@/lib/launcherToolVisibility";

type MoreToolsHubProps = {
  activeTool: MoreToolsSection | null;
  onSelectTool: (tool: MoreToolsSection) => void;
  onBack: () => void;
  backToLauncher?: boolean;
  jttParserTab?: JttParserTab;
};

export function MoreToolsHub({
  activeTool,
  onSelectTool,
  onBack,
  backToLauncher = false,
  jttParserTab,
}: MoreToolsHubProps) {
  const { i18n, t } = useTranslation();
  const [visibility, setVisibility] = useState<LauncherToolVisibility>(() =>
    readLauncherToolVisibility(),
  );
  const [toolOrder, setToolOrder] = useState<string[]>(() =>
    readSavedOrder(MORE_TOOLS_ORDER_KEY),
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
  const backLabel = backToLauncher
    ? i18n.language === "zh"
      ? "返回启动台"
      : "Back to Launcher"
    : i18n.language === "zh"
      ? "返回工具列表"
      : "Back to tools";

  const tools = useMemo(
    () => [
      {
        id: "bookmarks" as const,
        label: i18n.language === "zh" ? "书签" : "Bookmarks",
        description:
          i18n.language === "zh"
            ? "沉淀常用链接和资源入口"
            : "Save the links and resources you revisit often",
        launcherToolId: "bookmarks" as LauncherToolId,
      },
      {
        id: "cloud" as const,
        label: i18n.language === "zh" ? "云盘" : "Cloud Drive",
        description:
          i18n.language === "zh"
            ? "查看和整理云端文件内容"
            : "Browse and organize synced cloud files",
        launcherToolId: "cloud" as LauncherToolId,
      },
      {
        id: "ssh" as const,
        label: i18n.language === "zh" ? "SSH 服务器" : "SSH Servers",
        description:
          i18n.language === "zh"
            ? "集中管理 SSH 配置，快速连接远程主机"
            : "Open saved SSH hosts, history, and custom connections quickly",
        launcherToolId: "ssh" as LauncherToolId,
      },
      {
        id: "ssh-tunnels" as const,
        label: i18n.language === "zh" ? "SSH 隧道" : "SSH Tunnels",
        description:
          i18n.language === "zh"
            ? "管理本地、远程和 SOCKS5 动态 SSH 隧道"
            : "Manage local, remote, and dynamic SOCKS5 SSH tunnels",
        launcherToolId: "ssh-tunnels" as LauncherToolId,
      },
      {
        id: "protocol-router" as const,
        label: i18n.language === "zh" ? "协议路由" : "Protocol Router",
        description:
          i18n.language === "zh"
            ? "为 Claude Profile 和 OpenAI 兼容供应商暴露本地路由"
            : "Expose local Anthropic-compatible routes for Claude profiles",
        launcherToolId: "protocol-router" as LauncherToolId,
      },
      {
        id: "random-password" as const,
        label: t("randomPassword", "Random Password"),
        description: t("randomPasswordToolDesc", "Generate passwords locally with the character groups you need."),
        launcherToolId: "random-password" as LauncherToolId,
      },
      {
        id: "json-parser" as const,
        label: t("jsonParser", "JSON Parser"),
        description: t("jsonParserToolDesc", "Validate and format JSON locally in one editable workspace."),
        launcherToolId: "json-parser" as LauncherToolId,
      },
      {
        id: "md5-encryption" as const,
        label: t("md5Encryption.title", "MD5 Encryption"),
        description: t(
          "md5Encryption.description",
          "Calculate common MD5 hash formats locally from text.",
        ),
        launcherToolId: "md5Encryption" as LauncherToolId,
      },
      {
        id: "short-link" as const,
        label: t("shortLink", "Short Link"),
        description: t(
          "shortLinkToolDesc",
          "Create TinyURL short links and keep recent results on this device.",
        ),
        launcherToolId: "short-link" as LauncherToolId,
      },
      {
        id: "file-sharing" as const,
        label: t("fileSharing", "File Sharing"),
        description: t("fileSharingLauncherDesc", "Share selected files on a trusted local network."),
        launcherToolId: "file-sharing" as LauncherToolId,
      },
      {
        id: "jtt-data-parser" as const,
        label: i18n.language === "zh" ? "JT/T 数据解析" : "JT/T Data Parser",
        description:
          i18n.language === "zh"
            ? "本地解析 JT/T 808、809、1078 报文并转换十六进制。"
            : "Parse JT/T 808, 809, 1078 packets and convert hex locally.",
        launcherToolId: "jtt-data-parser" as LauncherToolId,
      },
    ],
    [i18n.language, t],
  );

  const orderedTools = useMemo(
    () => applySavedOrder(tools, toolOrder),
    [tools, toolOrder],
  );

  const drag = useCardDragReorder({
    ids: orderedTools.map((tool) => tool.id),
    onReorder: (next) => {
      setToolOrder(next);
      writeSavedOrder(MORE_TOOLS_ORDER_KEY, next);
    },
  });

  const showInLauncherLabel =
    i18n.language === "zh" ? "在启动台展示" : "Show in Launcher";
  const hideInLauncherLabel =
    i18n.language === "zh" ? "不在启动台展示" : "Hide from Launcher";
  const activeToolConfig = tools.find((tool) => tool.id === activeTool);

  if (activeTool) {
    return (
      <div className="flex h-full min-h-0 flex-col gap-5">
        <div className="flex items-center justify-between gap-3">
          <button
            type="button"
            onClick={onBack}
            aria-label={backLabel}
            className="inline-flex h-9 items-center gap-2 rounded-md border px-3 text-sm font-medium hover:bg-muted"
          >
            <ArrowLeft className="h-4 w-4" />
            {backLabel}
          </button>
          {activeToolConfig?.launcherToolId ? (
            <div className="flex items-center gap-3">
              <span className="text-sm font-medium">{showInLauncherLabel}</span>
              <Switch
                aria-label={showInLauncherLabel}
                checked={visibility[activeToolConfig.launcherToolId]}
                onCheckedChange={() =>
                  handleToggleVisibility(activeToolConfig.launcherToolId)
                }
                title={
                  visibility[activeToolConfig.launcherToolId]
                    ? hideInLauncherLabel
                    : showInLauncherLabel
                }
              />
            </div>
          ) : null}
        </div>

        <div className="min-h-0 flex-1 overflow-y-auto">
          {activeTool === "bookmarks" ? <Bookmarks /> : null}
          {activeTool === "cloud" ? <CloudDrive /> : null}
          {activeTool === "ssh" ? <SshServers /> : null}
          {activeTool === "ssh-tunnels" ? <SshTunnels isVisible /> : null}
          {activeTool === "protocol-router" ? <ProtocolRouterTool isVisible /> : null}
          {activeTool === "random-password" ? <RandomPasswordTool /> : null}
          {activeTool === "json-parser" ? <JsonParserTool /> : null}
          {activeTool === "md5-encryption" ? <Md5EncryptionTool /> : null}
          {activeTool === "short-link" ? <ShortLinkTool /> : null}
          {activeTool === "file-sharing" ? <FileSharingTool isVisible /> : null}
          {activeTool === "jtt-data-parser" ? (
            <JttDataParserTool
              key={jttParserTab ?? "jt808"}
              initialTab={jttParserTab}
            />
          ) : null}
        </div>
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col gap-5">
      <div>
        <h3 className="text-xl font-bold tracking-tight">{moreToolsLabel}</h3>
        <p className="mt-1 text-sm text-muted-foreground">
          {i18n.language === "zh"
            ? "把仍然低频的辅助工具收在一起，保持左侧工具分组更聚焦。"
            : "Keep the lower-frequency support tools here so the sidebar stays focused."}
        </p>
      </div>

      {drag.isArrangeMode ? (
        <div className="flex items-center justify-between rounded-lg border border-primary/30 bg-primary/5 px-4 py-2">
          <span className="text-sm font-medium text-primary">
            {t("launcherDragToReorderHint", "Drag cards to reorder")}
          </span>
          <button
            type="button"
            onClick={drag.exitArrangeMode}
            className="rounded-md bg-primary px-3 py-1.5 text-sm font-medium text-primary-foreground hover:bg-primary/90"
          >
            {t("launcherDragReorderDone", "Done")}
          </button>
        </div>
      ) : null}

      <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-4">
        {orderedTools
          .filter(
            (tool) =>
              tool.id !== "md5-encryption" || visibility.md5Encryption,
          )
          .map((tool) => {
            const { icon: Icon, iconClassName } =
              getMoreToolPresentation(tool.id);
            const isDragging = drag.draggingId === tool.id;
            const isDragOver =
              drag.dragOverId === tool.id &&
              drag.draggingId &&
              !isDragging;

            return (
              <div key={tool.id} className="relative">
                <button
                  type="button"
                  onClick={() => {
                    if (drag.isArrangeMode) return;
                    onSelectTool(tool.id);
                  }}
                  onPointerOver={() => drag.handleCardPointerOver(tool.id)}
                  data-testid={`more-tool-card-${tool.id}`}
                  className={`group flex min-h-36 w-full flex-col justify-between rounded-xl border bg-card p-4 text-left shadow-sm transition-all hover:border-primary/50 hover:shadow-md ${
                    isDragging ? "opacity-60" : ""
                  } ${isDragOver ? "ring-2 ring-primary" : ""}`}
                >
                  <div className="flex items-start">
                    <div
                      className={`rounded-lg p-2 ${iconClassName}`}
                      data-testid={`more-tool-icon-${tool.id}`}
                    >
                      <Icon className="h-6 w-6" />
                    </div>
                  </div>
                  <div className="space-y-1">
                    <div className="font-semibold">{tool.label}</div>
                    <p className="text-sm leading-6 text-muted-foreground">
                      {tool.description}
                    </p>
                  </div>
                </button>
                <button
                  type="button"
                  aria-label={t(
                    "launcherDragToReorderHint",
                    "Drag cards to reorder",
                  )}
                  title={t(
                    "launcherDragToReorderHint",
                    "Drag cards to reorder",
                  )}
                  onPointerDown={(e) => drag.handlePointerDown(e, tool.id)}
                  onPointerUp={drag.handlePointerUp}
                  data-testid={`more-tool-drag-handle-${tool.id}`}
                  className="absolute right-2 top-2 cursor-grab touch-none rounded-md p-1 text-muted-foreground hover:text-foreground"
                >
                  <GripVertical className="h-4 w-4" />
                </button>
              </div>
            );
          })}
      </div>
    </div>
  );
}
