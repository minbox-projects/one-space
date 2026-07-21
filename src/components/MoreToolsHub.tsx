import { useMemo } from "react";
import { Cloud, Star } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Bookmarks } from "./Bookmarks";
import { CloudDrive } from "./CloudDrive";
import type { MoreToolsSection } from "@/lib/navigation";

type MoreToolsHubProps = {
  activeTool: MoreToolsSection;
  onSelectTool: (tool: MoreToolsSection) => void;
};

export function MoreToolsHub({
  activeTool,
  onSelectTool,
}: MoreToolsHubProps) {
  const { i18n } = useTranslation();
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
      },
      {
        id: "cloud" as const,
        label: i18n.language === "zh" ? "云盘" : "Cloud Drive",
        description:
          i18n.language === "zh"
            ? "查看和整理云端文件内容"
            : "Browse and organize synced cloud files",
        icon: Cloud,
      },
    ],
    [i18n.language],
  );

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
                <span className="rounded-full border bg-muted px-2 py-0.5 text-[10px] uppercase tracking-[0.16em] text-muted-foreground">
                  {i18n.language === "zh" ? "辅助工具" : "Utility"}
                </span>
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
      </div>
    </div>
  );
}
