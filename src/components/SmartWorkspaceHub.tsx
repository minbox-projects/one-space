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
    if (initialSection !== activeSection) {
      setActiveSection(initialSection);
    }
  }, [activeSection, initialSection]);

  const tabs = useMemo(
    () => [
      {
        id: "conversations" as const,
        label: i18n.language === "zh" ? "AI助手" : "AI Assistant",
        description:
          i18n.language === "zh"
            ? "保持原有助手对话体验"
            : "Keep the original assistant conversation experience",
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
    <div className="flex h-full flex-col gap-5">
      <div className="space-y-4">
        <div className="space-y-1">
          <h2 className="text-xl font-bold tracking-tight">
            {i18n.language === "zh" ? "智能工作台" : "Smart Workspace"}
          </h2>
          <p className="text-sm text-muted-foreground">
            {i18n.language === "zh"
              ? "在一个入口下切换 AI助手、助手库、自动化和模型中心。"
              : "Switch between AI assistant conversations, assistant library, automations, and model controls in one place."}
          </p>
        </div>

        <div className="grid gap-3 md:grid-cols-2 xl:grid-cols-4">
          {tabs.map((tab) => {
            const Icon = tab.icon;
            const selected = tab.id === activeSection;
            return (
              <button
                key={tab.id}
                type="button"
                onClick={() => setActiveSection(tab.id)}
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
                    <div className="text-sm font-medium">{tab.label}</div>
                    <div className="mt-1 text-xs leading-5 text-muted-foreground">
                      {tab.description}
                    </div>
                  </div>
                </div>
              </button>
            );
          })}
        </div>
      </div>

      <div className="min-h-0 flex-1 overflow-hidden">
        {activeSection === "conversations" ? <AiWorkspaceSimple /> : null}
        {activeSection === "assistants" ? <AiWorkspace mode="assistants" /> : null}
        {activeSection === "automations" ? (
          <AiWorkspace mode="automations" />
        ) : null}
        {activeSection === "models" ? <AiWorkspace mode="models" /> : null}
      </div>
    </div>
  );
}
