import { useState } from "react";
import {
  Bot,
  ChevronDown,
  MoreHorizontal,
  Plus,
  Settings,
  Zap,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import type { AssistantPreset } from "@/lib/aiWorkspace";

interface ChatTopBarProps {
  currentAssistantId: string | null;
  assistants: AssistantPreset[];
  onAssistantChange: (id: string) => void;
  onCreateTopic: () => void;
  onOpenQuickAssistant: () => void;
  onOpenAssistantLibrary: () => void;
  onNavigateToSection?: (section: "automations" | "models") => void;
}

export function ChatTopBar({
  currentAssistantId,
  assistants,
  onAssistantChange,
  onCreateTopic,
  onOpenQuickAssistant,
  onOpenAssistantLibrary,
  onNavigateToSection,
}: ChatTopBarProps) {
  const { t } = useTranslation();
  const [assistantDropdownOpen, setAssistantDropdownOpen] = useState(false);
  const [moreMenuOpen, setMoreMenuOpen] = useState(false);

  const currentAssistant = assistants.find((a) => a.id === currentAssistantId);

  return (
    <div className="flex items-center gap-3 border-b px-4 py-3">
      {/* 助手预设选择器 */}
      <div className="relative">
        <button
          type="button"
          onClick={() => setAssistantDropdownOpen(!assistantDropdownOpen)}
          className="inline-flex items-center gap-2 rounded-xl border bg-background px-3 py-2 text-sm hover:bg-muted"
        >
          <div className="rounded-full bg-primary/10 p-1.5 text-primary">
            {currentAssistant?.avatar_emoji ? (
              <span className="text-xs">{currentAssistant.avatar_emoji}</span>
            ) : (
              <Bot className="h-3.5 w-3.5" />
            )}
          </div>
          <span className="font-medium">
            {currentAssistant?.name || t("noPreset", "No preset")}
          </span>
          <ChevronDown className="h-3.5 w-3.5 text-muted-foreground" />
        </button>

        {assistantDropdownOpen ? (
          <div className="absolute left-0 top-full z-50 mt-1 min-w-[200px] rounded-xl border bg-card shadow-lg">
            <div className="max-h-[300px] overflow-y-auto p-2">
              {assistants.map((assistant) => (
                <button
                  key={assistant.id}
                  type="button"
                  onClick={() => {
                    onAssistantChange(assistant.id);
                    setAssistantDropdownOpen(false);
                  }}
                  className={`w-full rounded-lg px-3 py-2 text-left text-sm transition-colors ${
                    currentAssistantId === assistant.id
                      ? "bg-primary/5 text-primary"
                      : "hover:bg-muted"
                  }`}
                >
                  <div className="flex items-center gap-2">
                    <span className="text-xs">
                      {assistant.avatar_emoji || "🤖"}
                    </span>
                    <span className="font-medium">{assistant.name}</span>
                  </div>
                  {assistant.description ? (
                    <div className="mt-1 line-clamp-1 text-xs text-muted-foreground">
                      {assistant.description}
                    </div>
                  ) : null}
                </button>
              ))}
            </div>
          </div>
        ) : null}
      </div>

      {/* 新建主题按钮 */}
      <button
        type="button"
        onClick={onCreateTopic}
        title={t("createTopic", "Create Topic")}
        className="inline-flex h-9 w-9 items-center justify-center rounded-xl border hover:bg-muted"
      >
        <Plus className="h-4 w-4" />
      </button>

      {/* Quick Assistant 按钮 */}
      <button
        type="button"
        onClick={onOpenQuickAssistant}
        title={t("quickAssistant", "Quick Assistant")}
        className="inline-flex h-9 w-9 items-center justify-center rounded-xl border hover:bg-muted"
      >
        <Zap className="h-4 w-4" />
      </button>

      {/* 助手库按钮 */}
      <button
        type="button"
        onClick={onOpenAssistantLibrary}
        title={t("assistantLibrary", "Assistant Library")}
        className="inline-flex h-9 w-9 items-center justify-center rounded-xl border hover:bg-muted"
      >
        <Bot className="h-4 w-4" />
      </button>

      {/* 更多菜单 */}
      <div className="relative ml-auto">
        <button
          type="button"
          onClick={() => setMoreMenuOpen(!moreMenuOpen)}
          className="inline-flex h-9 w-9 items-center justify-center rounded-xl border hover:bg-muted"
        >
          <MoreHorizontal className="h-4 w-4" />
        </button>

        {moreMenuOpen ? (
          <div className="absolute right-0 top-full z-50 mt-1 min-w-[180px] rounded-xl border bg-card shadow-lg">
            <div className="p-2">
              {onNavigateToSection ? (
                <>
                  <button
                    type="button"
                    onClick={() => {
                      onNavigateToSection("automations");
                      setMoreMenuOpen(false);
                    }}
                    className="w-full rounded-lg px-3 py-2 text-left text-sm hover:bg-muted"
                  >
                    {t("goToAutomations", "Go to Automations")}
                  </button>
                  <button
                    type="button"
                    onClick={() => {
                      onNavigateToSection("models");
                      setMoreMenuOpen(false);
                    }}
                    className="w-full rounded-lg px-3 py-2 text-left text-sm hover:bg-muted"
                  >
                    {t("goToModelCenter", "Go to Model Center")}
                  </button>
                </>
              ) : null}
              <button
                type="button"
                onClick={() => {
                  // TODO: 打开设置
                  setMoreMenuOpen(false);
                }}
                className="w-full rounded-lg px-3 py-2 text-left text-sm hover:bg-muted"
              >
                <div className="flex items-center gap-2">
                  <Settings className="h-3.5 w-3.5" />
                  <span>{t("settings", "Settings")}</span>
                </div>
              </button>
            </div>
          </div>
        ) : null}
      </div>
    </div>
  );
}