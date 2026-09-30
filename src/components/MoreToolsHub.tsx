import { useCallback, useEffect, useMemo, useState } from "react";
import type { ComponentType } from "react";
import { ArrowLeft, GripVertical } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Switch } from "./ui/switch";
import type { JttParserTab, MoreToolsSection } from "@/lib/navigation";
import {
  MORE_TOOLS_ORDER_KEY,
  applySavedOrder,
  readSavedOrder,
  writeSavedOrder,
} from "@/lib/launcherToolOrder";
import { useCardDragReorder } from "@/lib/useCardDragReorder";
import {
  LAUNCHER_TOOL_VISIBILITY_UPDATED_EVENT,
  readLauncherToolVisibility,
  setLauncherToolVisible,
  type LauncherToolVisibility,
} from "@/lib/launcherToolVisibility";
import {
  getToolboxTool,
  listToolboxTools,
  resolveToolboxText,
} from "@/toolbox/registry";

type MoreToolsHubProps = {
  activeTool: MoreToolsSection | null;
  onSelectTool: (tool: MoreToolsSection) => void;
  onBack: () => void;
  backToLauncher?: boolean;
  jttParserTab?: JttParserTab;
  sshTunnelTab?: string;
  sshTunnelNavigationNonce?: number;
  isVisible?: boolean;
};

const HUB_TOOLS = listToolboxTools("hub");

type ActiveToolComponent = ComponentType<{
  isVisible?: boolean;
  initialTab?: string;
  navigationNonce?: number;
}>;

export function MoreToolsHub({
  activeTool,
  onSelectTool,
  onBack,
  backToLauncher = false,
  jttParserTab,
  sshTunnelTab,
  sshTunnelNavigationNonce,
  isVisible = true,
}: MoreToolsHubProps) {
  const { i18n, t } = useTranslation();
  const [visibility, setVisibility] = useState<LauncherToolVisibility>(() =>
    readLauncherToolVisibility(),
  );
  const [toolOrder, setToolOrder] = useState<string[]>(() =>
    readSavedOrder(MORE_TOOLS_ORDER_KEY),
  );

  useEffect(() => {
    const refreshVisibility = () => {
      setVisibility(readLauncherToolVisibility());
    };
    window.addEventListener(
      LAUNCHER_TOOL_VISIBILITY_UPDATED_EVENT,
      refreshVisibility,
    );
    return () => {
      window.removeEventListener(
        LAUNCHER_TOOL_VISIBILITY_UPDATED_EVENT,
        refreshVisibility,
      );
    };
  }, []);

  const handleToggleVisibility = useCallback(
    (toolId: string) => {
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

  const orderedTools = useMemo(
    () => applySavedOrder(HUB_TOOLS, toolOrder),
    [toolOrder],
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
  const activeDescriptor = activeTool ? getToolboxTool(activeTool) : undefined;

  if (activeTool) {
    const ActiveComponent = activeDescriptor
      ? (activeDescriptor.component as ActiveToolComponent)
      : undefined;

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
          {activeDescriptor ? (
            <div className="flex items-center gap-3">
              <span className="text-sm font-medium">{showInLauncherLabel}</span>
              <Switch
                aria-label={showInLauncherLabel}
                checked={visibility[activeDescriptor.id]}
                onCheckedChange={() => handleToggleVisibility(activeDescriptor.id)}
                title={
                  visibility[activeDescriptor.id]
                    ? hideInLauncherLabel
                    : showInLauncherLabel
                }
              />
            </div>
          ) : null}
        </div>

        <div className="min-h-0 flex-1 overflow-y-auto">
          {ActiveComponent && activeDescriptor ? (
            <ActiveComponent
              key={
                activeDescriptor.id === "jtt-data-parser"
                  ? jttParserTab ?? "jt808"
                  : activeDescriptor.id
              }
              isVisible={isVisible && activeTool === activeDescriptor.id}
              {...(activeDescriptor.id === "jtt-data-parser"
                ? { initialTab: jttParserTab }
                : {})}
              {...(activeDescriptor.id === "ssh-tunnels"
                ? {
                    initialTab: sshTunnelTab,
                    navigationNonce: sshTunnelNavigationNonce,
                  }
                : {})}
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

      <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-4">
        {orderedTools.map((tool) => {
          const Icon = tool.icon;
          const isDragging = drag.draggingId === tool.id;

          return (
            <div key={tool.id} className="relative">
              <button
                type="button"
                onClick={() => onSelectTool(tool.id as MoreToolsSection)}
                onPointerOver={() => drag.handleCardPointerOver(tool.id)}
                data-testid={`more-tool-card-${tool.id}`}
                className={`group flex min-h-36 w-full flex-col justify-between rounded-xl border bg-card p-4 text-left shadow-sm transition-all hover:border-primary/50 hover:shadow-md ${
                  isDragging ? "ring-2 ring-primary" : ""
                }`}
              >
                <div className="flex items-start">
                  <div
                    className={`rounded-lg p-2 ${tool.iconClassName}`}
                    data-testid={`more-tool-icon-${tool.id}`}
                  >
                    <Icon className="h-6 w-6" />
                  </div>
                </div>
                <div className="space-y-1">
                  <div className="font-semibold">
                    {resolveToolboxText(tool.labelText, tool.labelKey, t)}
                  </div>
                  <p className="text-sm leading-6 text-muted-foreground">
                    {resolveToolboxText(
                      tool.descriptionText,
                      tool.descriptionKey,
                      t,
                    )}
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
