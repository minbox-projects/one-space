import { Bell, Clock, Plus, Server, X } from "lucide-react";
import { useTranslation } from "react-i18next";

import { Switch } from "@/components/ui/switch";
import { SettingCard } from "../common/SettingCard";
import {
  isValidDailyTime,
  type ServerMonitorScheduleMode,
  type ServerMonitorSettings,
} from "@/lib/serverMonitor";

function BiText({
  translationKey,
  values,
}: {
  translationKey: string;
  values?: Record<string, string | number>;
}) {
  const { t, i18n } = useTranslation();
  const language = i18n.resolvedLanguage || i18n.language || "zh";
  const alternate = language.startsWith("zh") ? "en" : "zh";
  const primary = t(translationKey, values);
  const secondary = i18n.getFixedT(alternate)(translationKey, values);
  return (
    <>
      {primary}
      {secondary && secondary !== primary ? (
        <span className="sr-only"> {secondary}</span>
      ) : null}
    </>
  );
}

export interface ServerMonitorSettingsViewProps {
  settings: ServerMonitorSettings;
  onSettingsChange: (settings: ServerMonitorSettings) => void;
  intervalInput: string;
  onIntervalInputChange: (value: string) => void;
  retentionInput: string;
  onRetentionInputChange: (value: string) => void;
  newTimeInput: string;
  onNewTimeInputChange: (value: string) => void;
  cpuThresholdInput?: string;
  onCpuThresholdInputChange?: (value: string) => void;
  memoryThresholdInput?: string;
  onMemoryThresholdInputChange?: (value: string) => void;
  diskThresholdInput?: string;
  onDiskThresholdInputChange?: (value: string) => void;
  alertCooldownInput?: string;
  onAlertCooldownInputChange?: (value: string) => void;
  onShowToast?: (type: "success" | "error", text: string) => void;
}

export function ServerMonitorSettingsView({
  settings,
  onSettingsChange,
  intervalInput,
  onIntervalInputChange,
  retentionInput,
  onRetentionInputChange,
  newTimeInput,
  onNewTimeInputChange,
  cpuThresholdInput,
  onCpuThresholdInputChange,
  memoryThresholdInput,
  onMemoryThresholdInputChange,
  diskThresholdInput,
  onDiskThresholdInputChange,
  alertCooldownInput,
  onAlertCooldownInputChange,
  onShowToast,
}: ServerMonitorSettingsViewProps) {
  const { t, i18n } = useTranslation();
  const language = i18n.resolvedLanguage || i18n.language || "zh";
  const alternate = language.startsWith("zh") ? "en" : "zh";
  const alt = (key: string) => i18n.getFixedT(alternate)(key);

  const cpuVal = cpuThresholdInput ?? String(settings.cpuThresholdPercent);
  const memVal = memoryThresholdInput ?? String(settings.memoryThresholdPercent);
  const diskVal = diskThresholdInput ?? String(settings.diskThresholdPercent);
  const cooldownVal = alertCooldownInput ?? String(settings.alertCooldownMinutes);

  const setMode = (scheduleMode: ServerMonitorScheduleMode) => {
    onSettingsChange({ ...settings, scheduleMode });
  };

  const addTime = () => {
    const value = newTimeInput.trim();
    if (!isValidDailyTime(value)) {
      onShowToast?.("error", `${t("serverMonitorTimeInvalid")} ${alt("serverMonitorTimeInvalid")}`);
      return;
    }
    if (settings.dailyTimes.includes(value)) {
      onShowToast?.("error", `${t("serverMonitorTimeDuplicate")} ${alt("serverMonitorTimeDuplicate")}`);
      return;
    }
    onSettingsChange({
      ...settings,
      dailyTimes: [...settings.dailyTimes, value].sort(),
    });
    onNewTimeInputChange("");
  };

  const removeTime = (time: string) => {
    if (settings.dailyTimes.length <= 1) {
      onShowToast?.("error", `${t("serverMonitorDailyEmpty")} ${alt("serverMonitorDailyEmpty")}`);
      return;
    }
    onSettingsChange({
      ...settings,
      dailyTimes: settings.dailyTimes.filter((item) => item !== time),
    });
  };

  return (
    <>
    <SettingCard
      id="section-server-monitor"
      icon={Server}
      title={<BiText translationKey="serverMonitorScheduleTitle" />}
      description={<BiText translationKey="serverMonitorScheduleDesc" />}
    >
      <div className="flex items-center justify-between gap-4">
        <div className="space-y-0.5">
          <h3 className="text-sm font-medium text-foreground">
            <BiText translationKey="serverMonitorAutoCollect" />
          </h3>
          <p className="text-xs text-muted-foreground">
            <BiText translationKey="serverMonitorAutoCollectDesc" />
          </p>
        </div>
        <Switch
          aria-label={`${t("serverMonitorAutoCollect")} ${alt("serverMonitorAutoCollect")}`}
          checked={settings.autoCollectEnabled}
          onCheckedChange={(checked) =>
            onSettingsChange({ ...settings, autoCollectEnabled: checked })
          }
        />
      </div>

      <hr className="border-border/50" />

      <fieldset className="space-y-2">
        <legend className="text-sm font-medium text-foreground">
          <BiText translationKey="serverMonitorScheduleMode" />
        </legend>
        <div className="flex flex-wrap gap-2">
          {(
            [
              ["interval", "serverMonitorModeRecurring"],
              ["daily", "serverMonitorModeDaily"],
            ] as const
          ).map(([mode, key]) => (
            <label
              key={mode}
              className={`inline-flex cursor-pointer items-center gap-2 rounded-lg border px-3 py-1.5 text-xs font-medium ${
                settings.scheduleMode === mode
                  ? "border-primary bg-primary/10 text-primary"
                  : "border-border bg-background text-muted-foreground"
              }`}
            >
              <input
                type="radio"
                name="server-monitor-schedule-mode"
                value={mode}
                checked={settings.scheduleMode === mode}
                onChange={() => setMode(mode)}
                className="sr-only"
              />
              <BiText translationKey={key} />
            </label>
          ))}
        </div>
      </fieldset>

      {settings.scheduleMode === "interval" ? (
        <div className="space-y-1.5">
          <label htmlFor="server-monitor-interval" className="text-sm font-medium text-foreground">
            <BiText translationKey="serverMonitorIntervalMinutes" />
          </label>
          <input
            id="server-monitor-interval"
            type="number"
            min={1}
            max={1440}
            value={intervalInput}
            onChange={(event) => onIntervalInputChange(event.target.value)}
            className="w-full rounded-xl border bg-background px-4 py-2 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-primary/20"
          />
          <p className="text-xs text-muted-foreground">
            <BiText translationKey="serverMonitorIntervalHint" />
          </p>
        </div>
      ) : (
        <div className="space-y-3">
          <div>
            <h3 className="text-sm font-medium text-foreground">
              <BiText translationKey="serverMonitorDailyTimes" />
            </h3>
            <p className="text-xs text-muted-foreground">
              <BiText translationKey="serverMonitorDailyTimesDesc" />
            </p>
          </div>

          <div className="flex flex-wrap items-center gap-2">
            {settings.dailyTimes.map((time) => (
              <span
                key={time}
                className="inline-flex items-center gap-1.5 rounded-lg border bg-muted/60 px-3 py-1 text-xs font-mono font-medium text-foreground"
              >
                <Clock className="h-3.5 w-3.5 text-muted-foreground" />
                {time}
                <button
                  type="button"
                  aria-label={`${t("serverMonitorRemoveTime", { time })} ${alt("serverMonitorRemoveTime")}`}
                  onClick={() => removeTime(time)}
                  className="ml-1 rounded p-0.5 text-muted-foreground transition-colors hover:bg-muted hover:text-rose-500"
                >
                  <X className="h-3 w-3" />
                </button>
              </span>
            ))}
          </div>

          <div className="flex items-center gap-2">
            <input
              type="time"
              value={newTimeInput}
              onChange={(event) => onNewTimeInputChange(event.target.value)}
              aria-label={`${t("serverMonitorAddTime")} ${alt("serverMonitorAddTime")}`}
              className="rounded-lg border bg-background px-3 py-1.5 text-xs font-mono text-foreground focus:outline-none focus:ring-2 focus:ring-primary/20"
            />
            <button
              type="button"
              onClick={addTime}
              className="inline-flex items-center gap-1 rounded-lg border bg-background px-3 py-1.5 text-xs font-medium text-foreground transition-colors hover:bg-muted"
            >
              <Plus className="h-3.5 w-3.5" />
              <BiText translationKey="serverMonitorAddTime" />
            </button>
          </div>
        </div>
      )}

      <hr className="border-border/50" />

      <div className="space-y-1.5">
        <label htmlFor="server-monitor-retention" className="text-sm font-medium text-foreground">
          <BiText translationKey="serverMonitorRetentionDays" />
        </label>
        <input
          id="server-monitor-retention"
          type="number"
          min={1}
          max={30}
          value={retentionInput}
          onChange={(event) => onRetentionInputChange(event.target.value)}
          className="w-full rounded-xl border bg-background px-4 py-2 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-primary/20"
        />
        <p className="text-xs text-muted-foreground">
          <BiText translationKey="serverMonitorRetentionHint" />
        </p>
        <p className="rounded-xl border border-amber-500/20 bg-amber-500/10 p-3 text-xs text-amber-700 dark:text-amber-400">
          <BiText translationKey="serverMonitorRetentionWarning" />
        </p>
      </div>
    </SettingCard>

    <SettingCard
      id="section-server-monitor-alerts"
      icon={Bell}
      title={<BiText translationKey="serverMonitorAlertTitle" />}
      description={<BiText translationKey="serverMonitorAlertDesc" />}
    >
      <div className="flex items-center justify-between gap-4">
        <div className="space-y-0.5">
          <h3 className="text-sm font-medium text-foreground">
            <BiText translationKey="serverMonitorAlertEnabled" />
          </h3>
          <p className="text-xs text-muted-foreground">
            <BiText translationKey="serverMonitorAlertEnabledDesc" />
          </p>
        </div>
        <Switch
          aria-label={`${t("serverMonitorAlertEnabled")} ${alt("serverMonitorAlertEnabled")}`}
          checked={settings.alertEnabled}
          onCheckedChange={(checked) =>
            onSettingsChange({ ...settings, alertEnabled: checked })
          }
        />
      </div>

      {settings.alertEnabled ? (
        <>
          <hr className="border-border/50" />

          <div className="grid grid-cols-1 gap-4 sm:grid-cols-3">
            <div className="space-y-1.5">
              <label htmlFor="server-monitor-cpu-threshold" className="text-sm font-medium text-foreground">
                <BiText translationKey="serverMonitorCpuThreshold" />
              </label>
              <div className="relative">
                <input
                  id="server-monitor-cpu-threshold"
                  type="number"
                  min={1}
                  max={100}
                  value={cpuVal}
                  onChange={(event) => {
                    onCpuThresholdInputChange?.(event.target.value);
                    const n = Number.parseInt(event.target.value, 10);
                    if (Number.isInteger(n) && n >= 1 && n <= 100) {
                      onSettingsChange({ ...settings, cpuThresholdPercent: n });
                    }
                  }}
                  className="w-full rounded-xl border bg-background px-4 py-2 pr-8 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-primary/20"
                />
                <span className="pointer-events-none absolute right-3 top-1/2 -translate-y-1/2 text-xs font-mono text-muted-foreground">
                  %
                </span>
              </div>
            </div>

            <div className="space-y-1.5">
              <label htmlFor="server-monitor-memory-threshold" className="text-sm font-medium text-foreground">
                <BiText translationKey="serverMonitorMemoryThreshold" />
              </label>
              <div className="relative">
                <input
                  id="server-monitor-memory-threshold"
                  type="number"
                  min={1}
                  max={100}
                  value={memVal}
                  onChange={(event) => {
                    onMemoryThresholdInputChange?.(event.target.value);
                    const n = Number.parseInt(event.target.value, 10);
                    if (Number.isInteger(n) && n >= 1 && n <= 100) {
                      onSettingsChange({ ...settings, memoryThresholdPercent: n });
                    }
                  }}
                  className="w-full rounded-xl border bg-background px-4 py-2 pr-8 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-primary/20"
                />
                <span className="pointer-events-none absolute right-3 top-1/2 -translate-y-1/2 text-xs font-mono text-muted-foreground">
                  %
                </span>
              </div>
            </div>

            <div className="space-y-1.5">
              <label htmlFor="server-monitor-disk-threshold" className="text-sm font-medium text-foreground">
                <BiText translationKey="serverMonitorDiskThreshold" />
              </label>
              <div className="relative">
                <input
                  id="server-monitor-disk-threshold"
                  type="number"
                  min={1}
                  max={100}
                  value={diskVal}
                  onChange={(event) => {
                    onDiskThresholdInputChange?.(event.target.value);
                    const n = Number.parseInt(event.target.value, 10);
                    if (Number.isInteger(n) && n >= 1 && n <= 100) {
                      onSettingsChange({ ...settings, diskThresholdPercent: n });
                    }
                  }}
                  className="w-full rounded-xl border bg-background px-4 py-2 pr-8 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-primary/20"
                />
                <span className="pointer-events-none absolute right-3 top-1/2 -translate-y-1/2 text-xs font-mono text-muted-foreground">
                  %
                </span>
              </div>
            </div>
          </div>

          <p className="text-xs text-muted-foreground">
            <BiText translationKey="serverMonitorThresholdHint" />
          </p>

          <hr className="border-border/50" />

          <div className="space-y-1.5">
            <label htmlFor="server-monitor-alert-cooldown" className="text-sm font-medium text-foreground">
              <BiText translationKey="serverMonitorAlertCooldownMinutes" />
            </label>
            <input
              id="server-monitor-alert-cooldown"
              type="number"
              min={1}
              max={1440}
              value={cooldownVal}
              onChange={(event) => {
                onAlertCooldownInputChange?.(event.target.value);
                const n = Number.parseInt(event.target.value, 10);
                if (Number.isInteger(n) && n >= 1 && n <= 1440) {
                  onSettingsChange({ ...settings, alertCooldownMinutes: n });
                }
              }}
              className="w-full rounded-xl border bg-background px-4 py-2 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-primary/20"
            />
            <p className="text-xs text-muted-foreground">
              <BiText translationKey="serverMonitorAlertCooldownHint" />
            </p>
          </div>
        </>
      ) : null}
    </SettingCard>
    </>
  );
}
