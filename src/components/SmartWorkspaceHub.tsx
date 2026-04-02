import { useEffect, useMemo, useState } from "react";
import { Bot, Clock3, Layers3, MessageSquare } from "lucide-react";
import { useTranslation } from "react-i18next";
import { AiWorkspace } from "./AiWorkspace";
import { AiWorkspaceSimple } from "./AiWorkspace/AiWorkspaceSimple";
import type { SmartWorkspaceSection } from "@/lib/navigation";

type SmartWorkspaceHubProps = {
  initialSection?: SmartWorkspaceSection;
};

export function SmartWorkspaceHub({
  initialSection = "conversations",
}: SmartWorkspaceHubProps) {
  const { i18n } = useTranslation();
  const [activeSection, setActiveSection] =
    useState<SmartWorkspaceSection>(initialSection);

  useEffect(() => {
    setActiveSection(initialSection);
  }, [initialSection]);

  const tabs = useMemo(
    () => [
      {
        id: "conversations" as const,
        label: i18n.language === "zh" ? "AI 对话" : "AI Chat",
        description:
          i18n.language === "zh"
            ? "进入助手对话与上下文能力"
            : "Open chats with context and assistant tools",
        icon: MessageSquare,
      },
      {
        id: "assistants" as const,
        label: i18n.language === "zh" ? "助手库" : "Assistant Library",
        description:
          i18n.language === "zh"
            ? "统一管理提示词、模型与能力绑定"
            : "Manage reusable prompts, models, and capability bindings",
        icon: Bot,
      },
      {
        id: "automations" as const,
        label: i18n.language === "zh" ? "自动化" : "Automations",
        description:
          i18n.language === "zh"
            ? "管理后台任务、触发器与运行记录"
            : "Manage background jobs, triggers, and recent runs",
        icon: Clock3,
      },
      {
        id: "models" as const,
        label: i18n.language === "zh" ? "模型中心" : "Model Center",
        description:
          i18n.language === "zh"
            ? "管理模型目录、连接与角色绑定"
            : "Manage model catalogs, connections, and role bindings",
        icon: Layers3,
      },
    ],
    [i18n.language],
  );

  return (
    <div className="flex h-full flex-col gap-4">
      <div className="flex flex-col gap-3 md:flex-row md:items-start md:justify-between">
        <div className="min-w-0 flex-1">
          <h2 className="text-xl font-bold tracking-tight">
            {i18n.language === "zh" ? "AI 工作台" : "AI Workspace"}
          </h2>
          <p className="mt-1 max-w-3xl text-sm text-muted-foreground">
            {i18n.language === "zh"
              ? "在一个入口下切换 AI 对话、助手库、自动化和模型中心。"
              : "Switch between AI chat, assistant library, automations, and model controls in one place."}
          </p>
        </div>

        <div className="flex min-w-0 shrink-0 flex-wrap items-center gap-2 md:justify-end">
          <div className="flex flex-wrap items-stretch gap-2">
            {tabs.map((tab) => {
              const Icon = tab.icon;
              const selected = tab.id === activeSection;
              return (
                <button
                  key={tab.id}
                  type="button"
                  onClick={() => setActiveSection(tab.id)}
                  title={tab.description}
                  aria-pressed={selected}
                  className={`min-w-[9.5rem] max-w-[11rem] rounded-2xl border px-3 py-3 text-left transition-colors ${
                    selected
                      ? "border-primary bg-primary/5"
                      : "bg-card hover:bg-muted/30"
                  }`}
                >
                  <div className="flex items-start gap-2.5">
                    <div
                      className={`rounded-xl p-1.5 ${
                        selected
                          ? "bg-primary/10 text-primary"
                          : "bg-muted text-muted-foreground"
                      }`}
                    >
                      <Icon className="h-4 w-4" />
                    </div>
                    <div className="min-w-0">
                      <div className="text-sm font-medium leading-5">
                        {tab.label}
                      </div>
                    </div>
                  </div>
                  <div className="mt-2 truncate text-[11px] leading-4 text-muted-foreground">
                    {tab.description}
                  </div>
                </button>
              );
            })}
          </div>
        </div>
      </div>

      <div className="min-h-0 flex-1 overflow-hidden">
        {activeSection === "conversations" ? <AiWorkspaceSimple /> : null}
        {activeSection === "assistants" ? (
          <AiWorkspace isVisible mode="assistants" />
        ) : null}
        {activeSection === "automations" ? (
          <AiWorkspace isVisible mode="automations" />
        ) : null}
        {activeSection === "models" ? (
          <AiWorkspace isVisible mode="models" />
        ) : null}
      </div>
    </div>
  );
}
