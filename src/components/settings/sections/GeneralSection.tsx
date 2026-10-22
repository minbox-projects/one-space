import { useTranslation } from "react-i18next";
import {
  Settings,
  Palette,
  Keyboard as KeyboardIcon,
  RefreshCw,
  CircleDot,
  Command,
  Zap,
  Moon,
  Sun,
  Monitor,
} from "lucide-react";
import { Switch } from "@/components/ui/switch";
import { SettingCard } from "../common/SettingCard";
import { SettingRow } from "../common/SettingRow";
import { FolderPicker } from "../common/FolderPicker";
import { RetentionChips } from "../common/RetentionChips";
import { SubTabNavigation, type SubTabItem } from "../common/SubTabNavigation";
import type { StorageConfig, LegacySettingsSection } from "../types";

export interface GeneralSectionProps {
  subTab: LegacySettingsSection;
  onSubTabChange: (tab: LegacySettingsSection) => void;
  config: StorageConfig;
  setConfig: React.Dispatch<React.SetStateAction<StorageConfig>>;
  theme: string;
  setTheme: (theme: any) => void;
  recordingField: "main" | "quick" | null;
  setRecordingField: (field: "main" | "quick" | null) => void;
  onCheckUpdatesNow?: () => void;
  checkingUpdates?: boolean;
  subTabDirtyMap?: Record<string, boolean>;
}

export function GeneralSection({
  subTab,
  onSubTabChange,
  config,
  setConfig,
  theme,
  setTheme,
  recordingField,
  setRecordingField,
  onCheckUpdatesNow,
  checkingUpdates = false,
  subTabDirtyMap = {},
}: GeneralSectionProps) {
  const { t, i18n } = useTranslation();

  const subTabItems: SubTabItem[] = [
    {
      id: "general",
      titleKey: "general",
      fallbackTitle: "基础设置",
      icon: Settings,
      dirty: subTabDirtyMap["general"],
    },
    {
      id: "appearance",
      titleKey: "appearance",
      fallbackTitle: "外观与语言",
      icon: Palette,
      dirty: subTabDirtyMap["appearance"],
    },
    {
      id: "shortcuts",
      titleKey: "shortcuts",
      fallbackTitle: "全局快捷键",
      icon: KeyboardIcon,
      dirty: subTabDirtyMap["shortcuts"],
    },
    {
      id: "updates",
      titleKey: "updates",
      fallbackTitle: "软件更新",
      icon: RefreshCw,
      dirty: subTabDirtyMap["updates"],
    },
  ];

  const toggleLanguage = async () => {
    const newLang = i18n.language === "zh" ? "en" : "zh";
    await i18n.changeLanguage(newLang);
    setConfig((prev) => ({ ...prev, language: newLang }));
  };

  const cycleTheme = () => {
    if (theme === "system") setTheme("dark");
    else if (theme === "dark") setTheme("light");
    else setTheme("system");
  };

  const ThemeIcon = theme === "dark" ? Moon : theme === "light" ? Sun : Monitor;

  return (
    <div className="space-y-6">
      <SubTabNavigation
        items={subTabItems}
        activeSubTab={subTab}
        onChange={onSubTabChange}
      />

      {/* 1. General Base Settings */}
      {subTab === "general" && (
        <SettingCard
          id="section-general"
          icon={Settings}
          title={t("general", "General")}
          description={t(
            "generalDesc",
            "Configure app-wide behavior settings.",
          )}
        >
          <SettingRow
            label={t("launchAtLogin", "Launch at Login")}
            description={t(
              "launchAtLoginDesc",
              "Start OneSpace automatically after system login, and keep it ready in the menu bar.",
            )}
          >
            <Switch
              checked={!!config.launch_at_login}
              onCheckedChange={(checked) =>
                setConfig((prev) => ({ ...prev, launch_at_login: checked }))
              }
            />
          </SettingRow>

          <hr className="border-border/50" />

          <SettingRow
            label={t("messageRetentionDays", "消息保留天数")}
            description={
              <div>
                <span>
                  {t(
                    "messageRetentionDaysDesc",
                    "Local message center keeps messages for this many days.",
                  )}
                </span>
                <p className="text-xs text-muted-foreground mt-0.5">
                  {t(
                    "messageRetentionDaysNote",
                    "Messages stay on this device and are not included in iCloud or Git sync.",
                  )}
                </p>
              </div>
            }
          >
            <RetentionChips
              value={config.message_retention_days ?? 30}
              onChange={(val) =>
                setConfig((prev) => ({
                  ...prev,
                  message_retention_days: val,
                }))
              }
              options={[7, 30, 90, 180]}
            />
          </SettingRow>

          <hr className="border-border/50" />

          <div className="space-y-2">
            <label className="text-sm font-medium text-muted-foreground">
              {t("defaultSftpDownloadDir", "SFTP 默认下载路径")}
            </label>
            <FolderPicker
              value={config.default_sftp_download_dir}
              placeholder={t(
                "defaultDownloadsDirPlaceholder",
                "默认下载至系统 Downloads 目录",
              )}
              onChange={(path) =>
                setConfig((prev) => ({
                  ...prev,
                  default_sftp_download_dir: path,
                }))
              }
              onReset={() =>
                setConfig((prev) => ({
                  ...prev,
                  default_sftp_download_dir: "",
                }))
              }
            />
            <p className="text-xs text-muted-foreground">
              {t(
                "defaultSftpDownloadDirDesc",
                "SFTP 工具下载远程文件时的本地存储路径，未配置时自动保存至系统 Downloads 文件夹。",
              )}
            </p>
          </div>
        </SettingCard>
      )}

      {/* 2. Appearance & Locale */}
      {subTab === "appearance" && (
        <SettingCard
          id="section-appearance"
          icon={Palette}
          title={t("appearance", "Appearance & Locale")}
          description={t(
            "appearanceDesc",
            "Customize how OneSpace looks and feels.",
          )}
        >
          <SettingRow
            label={t("theme", "App Theme")}
            description={t("themeDesc", "Select your preferred visual theme.")}
          >
            <button
              type="button"
              onClick={cycleTheme}
              className="flex items-center gap-2 px-4 py-2 bg-muted hover:bg-muted/80 rounded-xl transition-all"
            >
              <ThemeIcon className="w-4 h-4" />
              <span className="text-sm capitalize">{theme}</span>
            </button>
          </SettingRow>

          <hr className="border-border/50" />

          <SettingRow
            label={t("language", "Language")}
            description={t(
              "languageDesc",
              "Choose the language for the user interface.",
            )}
          >
            <button
              type="button"
              onClick={toggleLanguage}
              className="px-4 py-2 bg-muted hover:bg-muted/80 rounded-xl transition-all text-sm font-medium"
            >
              {i18n.language === "zh" ? "简体中文" : "English"}
            </button>
          </SettingRow>
        </SettingCard>
      )}

      {/* 3. Global Shortcuts */}
      {subTab === "shortcuts" && (
        <SettingCard
          id="section-shortcuts"
          icon={KeyboardIcon}
          title={t("shortcuts", "Global Shortcuts")}
          description={t(
            "shortcutsDesc",
            "Hotkeys to trigger OneSpace from anywhere.",
          )}
        >
          <div className="space-y-3">
            <label className="text-sm font-medium text-muted-foreground">
              {t("toggleMainWindow", "Toggle Main Window")}
            </label>
            <div className="flex items-center gap-4">
              <div
                className={`flex-1 flex items-center bg-muted/30 border rounded-xl px-4 py-4 text-sm transition-all h-14 ${
                  recordingField === "main"
                    ? "ring-2 ring-primary border-primary bg-primary/5"
                    : ""
                }`}
              >
                {recordingField === "main" ? (
                  <span className="flex items-center gap-3 text-primary font-bold animate-pulse">
                    <CircleDot className="w-4 h-4" />
                    {t("recordingPlaceholder", "Press keys...")}
                  </span>
                ) : (
                  <div className="flex gap-1.5 flex-wrap">
                    {config.main_shortcut?.split("+").map((key, i) => (
                      <kbd
                        key={i}
                        className="px-2.5 py-1 bg-background border-b-2 border-x border-t rounded-md font-mono text-sm shadow-sm"
                      >
                        {key === "Control" ? (
                          <Command className="w-3 h-3 inline mr-1" />
                        ) : null}
                        {key}
                      </kbd>
                    ))}
                  </div>
                )}
              </div>
              <button
                type="button"
                onClick={() =>
                  setRecordingField(recordingField === "main" ? null : "main")
                }
                className={`px-6 h-14 rounded-xl text-sm font-semibold transition-all active:scale-95 ${
                  recordingField === "main"
                    ? "bg-destructive text-destructive-foreground hover:bg-destructive/90 shadow-lg shadow-destructive/20"
                    : "bg-secondary text-secondary-foreground hover:bg-secondary/80"
                }`}
              >
                {recordingField === "main"
                  ? t("stopRecording", "Stop")
                  : t("record", "Record")}
              </button>
            </div>
          </div>

          <div className="space-y-3">
            <label className="text-sm font-medium text-muted-foreground">
              {t("toggleQuickAi", "Quick AI Session Bar")}
            </label>
            <div className="flex items-center gap-4">
              <div
                className={`flex-1 flex items-center bg-muted/30 border rounded-xl px-4 py-4 text-sm transition-all h-14 ${
                  recordingField === "quick"
                    ? "ring-2 ring-primary border-primary bg-primary/5"
                    : ""
                }`}
              >
                {recordingField === "quick" ? (
                  <span className="flex items-center gap-3 text-primary font-bold animate-pulse">
                    <CircleDot className="w-4 h-4" />
                    {t("recordingPlaceholder", "Press keys...")}
                  </span>
                ) : (
                  <div className="flex gap-1.5 flex-wrap">
                    {config.quick_ai_shortcut?.split("+").map((key, i) => (
                      <kbd
                        key={i}
                        className="px-2.5 py-1 bg-background border-b-2 border-x border-t rounded-md font-mono text-sm shadow-sm"
                      >
                        {key === "Control" ? (
                          <Command className="w-3 h-3 inline mr-1" />
                        ) : null}
                        {key}
                      </kbd>
                    ))}
                  </div>
                )}
              </div>
              <button
                type="button"
                onClick={() =>
                  setRecordingField(recordingField === "quick" ? null : "quick")
                }
                className={`px-6 h-14 rounded-xl text-sm font-semibold transition-all active:scale-95 ${
                  recordingField === "quick"
                    ? "bg-destructive text-destructive-foreground hover:bg-destructive/90 shadow-lg shadow-destructive/20"
                    : "bg-secondary text-secondary-foreground hover:bg-secondary/80"
                }`}
              >
                {recordingField === "quick"
                  ? t("stopRecording", "Stop")
                  : t("record", "Record")}
              </button>
            </div>
          </div>

          <div className="p-4 bg-primary/5 rounded-xl border border-primary/10 flex gap-3">
            <Zap className="w-5 h-5 text-primary shrink-0 mt-0.5" />
            <p className="text-xs text-primary/80 leading-relaxed italic">
              {t(
                "shortcutsNote",
                "Tip: You can use combinations like Command+Shift+K or Alt+Space.",
              )}
            </p>
          </div>
        </SettingCard>
      )}

      {/* 4. Software Updates */}
      {subTab === "updates" && (
        <SettingCard
          id="section-updates"
          icon={RefreshCw}
          title={t("updates", "Updates")}
          description={t(
            "updatesDesc",
            "Configure automatic version checks and background update downloads.",
          )}
          headerAction={
            onCheckUpdatesNow && (
              <button
                type="button"
                onClick={onCheckUpdatesNow}
                disabled={checkingUpdates}
                className="px-3 py-1.5 rounded-xl bg-secondary text-secondary-foreground text-xs font-medium hover:bg-secondary/80 inline-flex items-center gap-1.5 transition-all disabled:opacity-50"
              >
                <RefreshCw
                  className={`w-3.5 h-3.5 ${checkingUpdates ? "animate-spin" : ""}`}
                />
                <span>{t("checkNow", "立即检查")}</span>
              </button>
            )
          }
        >
          <SettingRow
            label={t("autoUpdate", "Automatic Updates")}
            description={t(
              "autoUpdateDesc",
              "When enabled, OneSpace will silently check and download updates in the background.",
            )}
          >
            <Switch
              checked={!!config.auto_update_enabled}
              onCheckedChange={(checked) =>
                setConfig((prev) => ({
                  ...prev,
                  auto_update_enabled: checked,
                }))
              }
            />
          </SettingRow>

          <hr className="border-border/50" />

          <div className="space-y-2">
            <label className="text-sm font-medium text-muted-foreground">
              {t("updateCheckFrequency", "Check Frequency (minutes)")}
            </label>
            <input
              type="number"
              min={30}
              max={1440}
              step={10}
              className="w-full bg-background border rounded-xl px-4 py-2.5 text-sm focus:outline-none focus:ring-2 focus:ring-primary/20"
              value={config.update_check_interval_minutes ?? 360}
              onChange={(e) => {
                const raw = parseInt(e.target.value, 10);
                const value = Number.isFinite(raw)
                  ? Math.max(30, Math.min(1440, raw))
                  : 360;
                setConfig((prev) => ({
                  ...prev,
                  update_check_interval_minutes: value,
                }));
              }}
            />
            <p className="text-xs text-muted-foreground">
              {t(
                "updateCheckFrequencyDesc",
                "Recommended range: 30 to 1440 minutes.",
              )}
            </p>
          </div>
        </SettingCard>
      )}
    </div>
  );
}
