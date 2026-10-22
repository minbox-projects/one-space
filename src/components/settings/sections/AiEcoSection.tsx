import { useTranslation } from "react-i18next";
import { Terminal, Sparkles, Bot, FolderOpen } from "lucide-react";
import { open } from "@tauri-apps/plugin-dialog";
import { skillModelOptions } from "@/components/skillsModelOptions";
import { SettingCard } from "../common/SettingCard";
import { FolderPicker } from "../common/FolderPicker";
import { RetentionChips } from "../common/RetentionChips";
import { SubTabNavigation, type SubTabItem } from "../common/SubTabNavigation";
import { EcoSourceManager } from "../common/EcoSourceManager";
import {
  type StorageConfig,
  type LegacySettingsSection,
  type AiModelId,
  type SkillsSyncState,
  type SubagentSourceDiagnoseResult,
  DEFAULT_AI_MODEL_LAUNCH_COMMANDS,
} from "../types";
import {
  normalizeAiModelLaunchCommandsForUi,
} from "../helpers";
import {
  normalizeAiModelPermissionModesForUi,
  type TerminalPermissionMode,
} from "@/lib/terminalPermissions";

export interface AiEcoSectionProps {
  subTab: LegacySettingsSection;
  onSubTabChange: (tab: LegacySettingsSection) => void;
  config: StorageConfig;
  setConfig: React.Dispatch<React.SetStateAction<StorageConfig>>;
  loading?: boolean;
  skillsSyncNowLoading?: boolean;
  onSkillsSyncNow?: () => Promise<void>;
  skillsSyncState?: SkillsSyncState | null;
  subagentsSyncNowLoading?: boolean;
  onSubagentsSyncNow?: () => Promise<void>;
  subagentsSyncState?: SkillsSyncState | null;
  subagentSourceDiagnosing?: Record<string, boolean>;
  subagentSourceDiagnostics?: Record<string, SubagentSourceDiagnoseResult>;
  onDiagnoseSubagentSource?: (sourceId: string) => Promise<void>;
  onShowToast?: (type: "success" | "error", text: string) => void;
  subTabDirtyMap?: Record<string, boolean>;
}

export function AiEcoSection({
  subTab,
  onSubTabChange,
  config,
  setConfig,
  loading = false,
  skillsSyncNowLoading = false,
  onSkillsSyncNow,
  skillsSyncState,
  subagentsSyncNowLoading = false,
  onSubagentsSyncNow,
  subagentsSyncState,
  subagentSourceDiagnosing,
  subagentSourceDiagnostics,
  onDiagnoseSubagentSource,
  onShowToast,
  subTabDirtyMap = {},
}: AiEcoSectionProps) {
  const { t } = useTranslation();

  const subTabItems: SubTabItem[] = [
    {
      id: "ai",
      titleKey: "aiSessions",
      fallbackTitle: "终端与模型",
      icon: Terminal,
      dirty: subTabDirtyMap["ai"],
    },
    {
      id: "skills",
      titleKey: "skillsSourcesMenu",
      fallbackTitle: "Skills 源",
      icon: Sparkles,
      dirty: subTabDirtyMap["skills"],
    },
    {
      id: "subagents",
      titleKey: "subagentsSourcesMenu",
      fallbackTitle: "Subagents 源",
      icon: Bot,
      dirty: subTabDirtyMap["subagents"],
    },
  ];

  const handleSelectTerminalApp = async () => {
    try {
      const selected = await open({
        directory: false,
        multiple: false,
        defaultPath: "/Applications",
        filters: [{ name: "Application", extensions: ["app"] }],
      });
      if (selected && typeof selected === "string") {
        const appName = selected.split("/").pop()?.replace(".app", "") || selected;
        setConfig((prev) => ({ ...prev, ai_terminal_app: appName }));
      }
    } catch (err: unknown) {
      console.error(err);
    }
  };

  return (
    <div className="space-y-6">
      <SubTabNavigation
        items={subTabItems}
        activeSubTab={subTab}
        onChange={onSubTabChange}
      />

      {/* 1. Terminal & Sessions Configuration */}
      {subTab === "ai" && (
        <SettingCard
          id="section-ai"
          icon={Terminal}
          title={t("aiSessions", "AI Terminal Sessions")}
          description={t(
            "aiSessionsDesc",
            "Default configuration for quick AI terminal sessions.",
          )}
        >
          {/* Default Model */}
          <div className="space-y-2">
            <label className="text-sm font-medium text-muted-foreground">
              {t("defaultAiModel", "Default Model")}
            </label>
            <div className="grid grid-cols-2 sm:grid-cols-4 gap-2">
              {skillModelOptions.map(({ id, label, Icon }) => {
                const active = (config.default_ai_model || "claude") === id;
                return (
                  <button
                    key={id}
                    type="button"
                    onClick={() =>
                      setConfig((prev) => ({
                        ...prev,
                        default_ai_model: id as StorageConfig["default_ai_model"],
                      }))
                    }
                    className={`flex items-center gap-2 rounded-xl border px-3 py-2.5 text-sm transition-all ${
                      active
                        ? "bg-primary text-primary-foreground border-primary shadow-sm"
                        : "bg-background hover:bg-muted/50 text-foreground border-border"
                    }`}
                  >
                    <Icon className="w-4 h-4 shrink-0" />
                    <span className="truncate">{label}</span>
                  </button>
                );
              })}
            </div>
            <p className="text-xs text-muted-foreground">
              {t(
                "defaultAiModelDesc",
                "Preselected model for the Quick AI Session Bar.",
              )}
            </p>
          </div>

          <hr className="border-border/50" />

          {/* Model Launch Commands and Permissions */}
          <div className="space-y-3">
            <label className="text-sm font-medium text-muted-foreground">
              {t("aiModelLaunchCommands", "Model Launch Commands")}
            </label>
            <div className="space-y-2.5">
              {skillModelOptions.map(({ id, label, Icon }) => {
                if (!["claude", "antigravity", "codex", "opencode"].includes(id)) {
                  return null;
                }
                const commands = normalizeAiModelLaunchCommandsForUi(
                  config.ai_model_launch_commands,
                );
                const permissionModes = normalizeAiModelPermissionModesForUi(
                  config.ai_model_permission_modes,
                );
                const modelId = id as AiModelId;

                return (
                  <div
                    key={`ai-launch-command-${id}`}
                    className="grid grid-cols-1 sm:grid-cols-[140px_1fr_140px] gap-2 items-center"
                  >
                    <div className="flex items-center gap-2 text-sm text-muted-foreground">
                      <Icon className="w-4 h-4 shrink-0" />
                      <span className="truncate font-medium">{label}</span>
                    </div>
                    <input
                      type="text"
                      value={commands[modelId]}
                      onChange={(e) =>
                        setConfig((prev) => ({
                          ...prev,
                          ai_model_launch_commands: {
                            ...normalizeAiModelLaunchCommandsForUi(
                              prev.ai_model_launch_commands,
                            ),
                            [modelId]: e.target.value,
                          },
                        }))
                      }
                      className="flex-1 bg-background border rounded-xl px-4 py-2.5 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-primary/20"
                      placeholder={DEFAULT_AI_MODEL_LAUNCH_COMMANDS[modelId]}
                    />
                    <select
                      value={permissionModes[modelId]}
                      onChange={(e) =>
                        setConfig((prev) => ({
                          ...prev,
                          ai_model_permission_modes: {
                            ...normalizeAiModelPermissionModesForUi(
                              prev.ai_model_permission_modes,
                            ),
                            [modelId]: e.target.value as TerminalPermissionMode,
                          },
                        }))
                      }
                      className="bg-background border rounded-xl px-3 py-2.5 text-sm focus:outline-none focus:ring-2 focus:ring-primary/20"
                    >
                      <option value="default">
                        {t("defaultPermissionMode", "Default")}
                      </option>
                      <option value="full_access">
                        {t("fullAccessPermissionMode", "Full Access")}
                      </option>
                    </select>
                  </div>
                );
              })}
            </div>
            <div className="space-y-1 text-xs text-muted-foreground pt-1">
              <p>
                {t(
                  "aiModelLaunchCommandsDesc",
                  "Used when creating a new AI terminal session. Supports {session_id} placeholder.",
                )}
              </p>
              <p>
                {t(
                  "terminalPermissionModeDesc",
                  "Controls permission behavior when resuming sessions. Full Access skips tool approval — used only for session recovery.",
                )}
              </p>
              <p className="text-amber-600/90 font-medium">
                {t(
                  "fullAccessRiskDesc",
                  "Skip tool permission checks or relax permission control.",
                )}
              </p>
            </div>
          </div>

          <hr className="border-border/50" />

          {/* Terminal Application */}
          <div className="space-y-2">
            <label className="text-sm font-medium text-muted-foreground">
              {t("aiTerminalApp", "Terminal Application")}
            </label>
            <div className="flex gap-2">
              <input
                type="text"
                readOnly
                value={
                  config.ai_terminal_app ||
                  t("aiTerminalAppPlaceholder", "终端")
                }
                className="flex-1 bg-muted/50 border rounded-xl px-4 py-2.5 text-sm text-muted-foreground cursor-default focus:outline-none"
              />
              <button
                type="button"
                onClick={handleSelectTerminalApp}
                className="px-4 py-2.5 bg-secondary text-secondary-foreground rounded-xl text-sm font-medium hover:bg-secondary/80 transition-all active:scale-95 flex items-center gap-1.5"
              >
                <FolderOpen className="w-4 h-4" />
                <span>{t("browse", "选择程序")}</span>
              </button>
            </div>
            <p className="text-xs text-muted-foreground">
              {t(
                "aiTerminalAppDesc",
                "Choose an app from Applications. OneSpace will use it to launch AI terminal sessions.",
              )}
            </p>
          </div>

          <hr className="border-border/50" />

          {/* Default Project Directory */}
          <div className="space-y-2">
            <label className="text-sm font-medium text-muted-foreground">
              {t("defaultAiPath", "Default Project Directory")}
            </label>
            <FolderPicker
              value={config.default_ai_dir}
              placeholder={t("notSet", "Not Set")}
              onChange={(path) =>
                setConfig((prev) => ({ ...prev, default_ai_dir: path }))
              }
              onReset={() =>
                setConfig((prev) => ({ ...prev, default_ai_dir: "" }))
              }
            />
          </div>

          <hr className="border-border/50" />

          {/* Claude Provider Launch Directory */}
          <div className="space-y-2">
            <label className="text-sm font-medium text-muted-foreground">
              {t("claudeProviderLaunchDir", "Claude Provider Launch Directory")}
            </label>
            <FolderPicker
              value={config.claude_provider_launch_dir}
              placeholder={t("notSet", "Not Set")}
              onChange={(path) =>
                setConfig((prev) => ({
                  ...prev,
                  claude_provider_launch_dir: path,
                }))
              }
              onReset={() =>
                setConfig((prev) => ({
                  ...prev,
                  claude_provider_launch_dir: "",
                }))
              }
            />
            <p className="text-xs text-muted-foreground">
              {t(
                "claudeProviderLaunchDirDesc",
                "Used only when launching Claude from AI service providers. Does not affect Quick AI or manual AI session creation.",
              )}
            </p>
          </div>

          <hr className="border-border/50" />

          {/* History Retention Days */}
          <div className="space-y-2">
            <label className="text-sm font-medium text-muted-foreground">
              {t("aiSessionsHistoryDays", "History Sync Duration (Days)")}
            </label>
            <RetentionChips
              value={config.ai_sessions_history_days ?? 30}
              onChange={(val) =>
                setConfig((prev) => ({
                  ...prev,
                  ai_sessions_history_days: val,
                }))
              }
              options={[7, 30, 90, 180]}
            />
            <p className="text-xs text-muted-foreground">
              {t(
                "aiSessionsHistoryDaysNote",
                "Sessions older than this will be hidden from the list",
              )}
            </p>
          </div>
        </SettingCard>
      )}

      {/* 2. Skills Sources */}
      {subTab === "skills" && (
        <EcoSourceManager
          type="skills"
          config={config}
          setConfig={setConfig}
          loading={loading}
          syncNowLoading={skillsSyncNowLoading}
          onSyncNow={onSkillsSyncNow}
          syncState={skillsSyncState}
          onShowToast={onShowToast}
        />
      )}

      {/* 3. Subagents Sources */}
      {subTab === "subagents" && (
        <EcoSourceManager
          type="subagents"
          config={config}
          setConfig={setConfig}
          loading={loading}
          syncNowLoading={subagentsSyncNowLoading}
          onSyncNow={onSubagentsSyncNow}
          syncState={subagentsSyncState}
          subagentSourceDiagnosing={subagentSourceDiagnosing}
          subagentSourceDiagnostics={subagentSourceDiagnostics}
          onDiagnoseSubagentSource={onDiagnoseSubagentSource}
          onShowToast={onShowToast}
        />
      )}
    </div>
  );
}
