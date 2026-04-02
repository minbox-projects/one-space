import { useMemo } from "react";
import { Code2, Cloud, NotebookPen, Star } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Bookmarks } from "./Bookmarks";
import { CloudDrive } from "./CloudDrive";
import { Notes } from "./Notes";
import { Snippets } from "./Snippets";
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
        id: "snippets" as const,
        label: i18n.language === "zh" ? "代码片段" : "Snippets",
        description:
          i18n.language === "zh"
            ? "集中管理复用代码与模板"
            : "Keep reusable code and templates in one place",
        icon: Code2,
      },
      {
        id: "notes" as const,
        label: i18n.language === "zh" ? "笔记" : "Notes",
        description:
          i18n.language === "zh"
            ? "记录临时想法、资料和备忘"
            : "Capture quick notes, references, and reminders",
        icon: NotebookPen,
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
      <div className="space-y-1">
        <h2 className="text-xl font-bold tracking-tight">{moreToolsLabel}</h2>
        <p className="text-sm text-muted-foreground">
          {i18n.language === "zh"
            ? "把低频但仍然重要的工具收在一起，避免左侧一级导航继续膨胀。"
            : "Group lower-frequency tools here so the primary sidebar stays focused."}
        </p>
      </div>

      <div className="grid gap-3 md:grid-cols-2 xl:grid-cols-3">
        {tools.map((tool) => {
          const Icon = tool.icon;
          const selected = tool.id === activeTool;
          return (
            <button
              key={tool.id}
              type="button"
              onClick={() => onSelectTool(tool.id)}
              className={`rounded-2xl border px-4 py-4 text-left transition-colors ${
                selected
                  ? "border-primary bg-primary/5"
                  : "bg-card hover:bg-muted/30"
              }`}
            >
              <div className="flex items-start gap-3">
                <div
                  className={`rounded-xl p-2 ${
                    selected
                      ? "bg-primary/10 text-primary"
                      : "bg-muted text-muted-foreground"
                  }`}
                >
                  <Icon className="h-4 w-4" />
                </div>
                <div className="min-w-0">
                  <div className="text-sm font-medium">{tool.label}</div>
                  <div className="mt-1 text-xs leading-5 text-muted-foreground">
                    {tool.description}
                  </div>
                </div>
              </div>
            </button>
          );
        })}
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto">
        {activeTool === "bookmarks" ? <Bookmarks /> : null}
        {activeTool === "snippets" ? <Snippets /> : null}
        {activeTool === "notes" ? <Notes /> : null}
        {activeTool === "cloud" ? <CloudDrive /> : null}
      </div>
    </div>
  );
}
