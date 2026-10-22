import { useState, useRef, useMemo } from "react";
import { useTranslation } from "react-i18next";
import {
  RefreshCw,
  Upload,
  Download,
  Plus,
  Trash2,
  Copy,
  Activity,
  AlertCircle,
  CheckCircle2,
  X,
} from "lucide-react";
import { save } from "@tauri-apps/plugin-dialog";
import { invoke } from "@tauri-apps/api/core";
import { skillModelOptions } from "@/components/skillsModelOptions";
import type {
  StorageConfig,
  SkillSourceConfig,
  SkillSourceValidation,
  SkillsSyncState,
  SubagentSourceDiagnoseResult,
} from "../types";
import {
  normalizeSkillSourcesForUi,
  DEFAULT_SKILL_SOURCE_MODELS,
} from "../helpers";

export interface EcoSourceManagerProps {
  type: "skills" | "subagents";
  config: StorageConfig;
  setConfig: React.Dispatch<React.SetStateAction<StorageConfig>>;
  loading?: boolean;
  syncNowLoading?: boolean;
  onSyncNow?: () => Promise<void>;
  syncState?: SkillsSyncState | null;
  subagentSourceDiagnosing?: Record<string, boolean>;
  subagentSourceDiagnostics?: Record<string, SubagentSourceDiagnoseResult>;
  onDiagnoseSubagentSource?: (sourceId: string) => Promise<void>;
  onShowToast?: (type: "success" | "error", text: string) => void;
}

export function EcoSourceManager({
  type,
  config,
  setConfig,
  loading = false,
  syncNowLoading = false,
  onSyncNow,
  syncState,
  subagentSourceDiagnosing = {},
  subagentSourceDiagnostics = {},
  onDiagnoseSubagentSource,
  onShowToast,
}: EcoSourceManagerProps) {
  const { t } = useTranslation();
  const fileInputRef = useRef<HTMLInputElement | null>(null);

  const isSkills = type === "skills";
  const syncEnabled = isSkills
    ? !!config.skills_sync_enabled
    : !!config.subagents_sync_enabled;
  const syncInterval = isSkills
    ? config.skills_sync_interval_minutes ?? 60
    : config.subagents_sync_interval_minutes ?? 60;
  const badgeHours = isSkills
    ? config.skills_new_badge_hours ?? 72
    : config.subagents_new_badge_hours ?? 72;
  const rawSources = isSkills ? config.skills_sources : config.subagents_sources;
  const sources = useMemo(
    () => normalizeSkillSourcesForUi(rawSources),
    [rawSources],
  );

  const [showAddModal, setShowAddModal] = useState(false);
  const [newSource, setNewSource] = useState<SkillSourceConfig>({
    id: "",
    name: "",
    repo_url: "",
    branch: "main",
    base_dir: "/",
    enabled: true,
    default_models: [...DEFAULT_SKILL_SOURCE_MODELS],
  });
  const [validation, setValidation] = useState<SkillSourceValidation>({});

  const syncSourceMap = useMemo(() => {
    const map = new Map<string, { last_synced_at?: number; last_status?: string; last_error?: string }>();
    if (syncState?.sources) {
      for (const item of syncState.sources) {
        map.set(item.source_id, item);
      }
    }
    return map;
  }, [syncState]);

  const enabledCount = useMemo(
    () => sources.filter((s) => s.enabled).length,
    [sources],
  );

  const lastSyncText = useMemo(() => {
    if (!syncState?.last_sync_at) return t("never", "Never");
    return new Date(syncState.last_sync_at).toLocaleString();
  }, [syncState, t]);

  const setSources = (
    updater: (prev: SkillSourceConfig[]) => SkillSourceConfig[],
  ) => {
    setConfig((prev) => {
      const current = isSkills
        ? normalizeSkillSourcesForUi(prev.skills_sources)
        : normalizeSkillSourcesForUi(prev.subagents_sources);
      const nextSources = updater(current);
      return isSkills
        ? { ...prev, skills_sources: nextSources }
        : { ...prev, subagents_sources: nextSources };
    });
  };

  const handleToggleSource = (sourceId: string, enabled: boolean) => {
    setSources((current) =>
      current.map((s) => (s.id === sourceId ? { ...s, enabled } : s)),
    );
  };

  const handleDeleteSource = (sourceId: string) => {
    setSources((current) => current.filter((s) => s.id !== sourceId));
  };

  const handleCopyRepo = async (url: string) => {
    try {
      await navigator.clipboard.writeText(url);
      onShowToast?.("success", t("copiedToClipboard", "Copied to clipboard"));
    } catch (e: any) {
      onShowToast?.("error", e?.toString() || "Copy failed");
    }
  };

  const validateNewSource = (draft: SkillSourceConfig): boolean => {
    const errors: SkillSourceValidation = {};
    if (!draft.id.trim()) {
      errors.id = t("fieldRequired", "This field is required");
    } else if (sources.some((s) => s.id === draft.id.trim())) {
      errors.id = t("idAlreadyExists", "ID already exists");
    }
    if (!draft.repo_url.trim()) {
      errors.repo_url = t("fieldRequired", "This field is required");
    }
    if (!draft.base_dir?.trim()) {
      errors.base_dir = t("fieldRequired", "This field is required");
    }
    setValidation(errors);
    return Object.keys(errors).length === 0;
  };

  const handleAddSource = () => {
    if (!validateNewSource(newSource)) return;
    setSources((current) => [
      ...current,
      {
        ...newSource,
        id: newSource.id.trim(),
        name: newSource.name.trim() || newSource.id.trim(),
        repo_url: newSource.repo_url.trim(),
        branch: (newSource.branch || "main").trim() || "main",
        base_dir: (newSource.base_dir || "/").trim() || "/",
        enabled: true,
      },
    ]);
    setShowAddModal(false);
    setNewSource({
      id: "",
      name: "",
      repo_url: "",
      branch: "main",
      base_dir: "/",
      enabled: true,
      default_models: [...DEFAULT_SKILL_SOURCE_MODELS],
    });
    setValidation({});
  };

  const toggleModelForNewSource = (modelId: string) => {
    setNewSource((prev) => {
      const current = prev.default_models || [];
      const exists = current.includes(modelId);
      return {
        ...prev,
        default_models: exists
          ? current.filter((m) => m !== modelId)
          : [...current, modelId],
      };
    });
  };

  const handleExport = async () => {
    try {
      const stamp = new Date().toISOString().replace(/[:.]/g, "-");
      const outputPath = await save({
        defaultPath: `${type}-sources-${stamp}.json`,
        filters: [{ name: "JSON", extensions: ["json"] }],
      });
      if (!outputPath || Array.isArray(outputPath)) return;
      await invoke(
        isSkills ? "skills_sources_export" : "subagents_sources_export",
        { outputPath },
      );
      onShowToast?.("success", t("exportSuccess", "Sources exported successfully"));
    } catch (e: any) {
      onShowToast?.("error", e?.toString?.() || "Export failed");
    }
  };

  const handleImport = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;
    try {
      const text = await file.text();
      const imported = JSON.parse(text);
      if (Array.isArray(imported)) {
        const normalized = normalizeSkillSourcesForUi(imported);
        setSources((current) => {
          const map = new Map(current.map((s) => [s.id, s]));
          for (const item of normalized) {
            map.set(item.id, item);
          }
          return Array.from(map.values());
        });
        onShowToast?.(
          "success",
          t("importSuccess", "Sources imported successfully"),
        );
      }
    } catch (err: any) {
      onShowToast?.("error", err?.toString?.() || "Import failed");
    } finally {
      if (fileInputRef.current) fileInputRef.current.value = "";
    }
  };

  return (
    <div className="space-y-6">
      {/* Header controls & sync now */}
      <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-3">
        <div className="space-y-1">
          <div className="flex items-center gap-2">
            <h3 className="text-base font-semibold">
              {isSkills
                ? t("skillsSourcesMenu", "Skills 源")
                : t("subagentsSourcesMenu", "Subagents 源")}
            </h3>
            <span className="text-xs px-2 py-0.5 rounded-full border bg-muted/40 font-mono">
              {enabledCount}/{sources.length}
            </span>
          </div>
          <p className="text-xs text-muted-foreground">
            {t("lastSyncAt", "Last Sync")}: {lastSyncText}
          </p>
        </div>

        {onSyncNow && (
          <button
            type="button"
            onClick={onSyncNow}
            disabled={loading || syncNowLoading}
            className="px-3.5 py-1.5 rounded-xl bg-primary text-primary-foreground text-xs font-medium hover:bg-primary/90 disabled:opacity-50 inline-flex items-center gap-2 shadow-sm transition-all self-start sm:self-auto"
          >
            {syncNowLoading ? (
              <RefreshCw className="w-3.5 h-3.5 animate-spin" />
            ) : (
              <RefreshCw className="w-3.5 h-3.5" />
            )}
            <span>{t("syncNow", "Sync Now")}</span>
          </button>
        )}
      </div>

      {/* Global Auto-Sync and Interval Configuration */}
      <div className="bg-card border rounded-2xl p-6 shadow-sm space-y-5">
        <div className="flex items-center justify-between">
          <div className="space-y-0.5">
            <h4 className="text-sm font-medium">
              {isSkills
                ? t("skillsSyncEnabled", "Enable Skills Auto Sync")
                : t("subagentsSyncEnabled", "Enable Subagents Auto Sync")}
            </h4>
            <p className="text-xs text-muted-foreground">
              {isSkills
                ? t(
                    "skillsSyncEnabledDesc",
                    "Global switch for scheduled Git repository skills sync.",
                  )
                : t(
                    "subagentsSyncEnabledDesc",
                    "Global switch for scheduled Git repository subagents sync.",
                  )}
            </p>
          </div>
          <label className="relative inline-flex items-center cursor-pointer">
            <input
              type="checkbox"
              className="sr-only peer"
              checked={syncEnabled}
              onChange={(e) =>
                setConfig((prev) =>
                  isSkills
                    ? { ...prev, skills_sync_enabled: e.target.checked }
                    : { ...prev, subagents_sync_enabled: e.target.checked },
                )
              }
            />
            <div className="w-11 h-6 bg-gray-200 peer-focus:outline-none peer-focus:ring-4 peer-focus:ring-primary/20 rounded-full peer dark:bg-gray-700 peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:border-gray-300 after:border after:rounded-full after:h-5 after:w-5 after:transition-all dark:border-gray-600 peer-checked:bg-primary"></div>
          </label>
        </div>

        {isSkills && (
          <div className="flex items-center justify-between border-t pt-4">
            <div className="space-y-0.5">
              <h4 className="text-sm font-medium">
                {t("skillsAutoUpdateEnabled", "Auto Update Skills")}
              </h4>
              <p className="text-xs text-muted-foreground">
                {t(
                  "skillsAutoUpdateEnabledDesc",
                  "After each scheduled source sync, automatically update changed repository skills.",
                )}
              </p>
            </div>
            <label className="relative inline-flex items-center cursor-pointer">
              <input
                type="checkbox"
                className="sr-only peer"
                checked={!!config.skills_auto_update_enabled}
                disabled={!syncEnabled}
                onChange={(e) =>
                  setConfig((prev) => ({
                    ...prev,
                    skills_auto_update_enabled: e.target.checked,
                  }))
                }
              />
              <div className="w-11 h-6 bg-gray-200 peer-focus:outline-none peer-focus:ring-4 peer-focus:ring-primary/20 rounded-full peer dark:bg-gray-700 peer-disabled:opacity-50 peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:border-gray-300 after:border after:rounded-full after:h-5 after:w-5 after:transition-all dark:border-gray-600 peer-checked:bg-primary"></div>
            </label>
          </div>
        )}

        <div className="grid grid-cols-1 sm:grid-cols-2 gap-4 border-t pt-4">
          <div className="space-y-1.5">
            <label className="text-xs font-medium text-muted-foreground">
              {isSkills
                ? t("skillsSyncInterval", "Skills Sync Interval (minutes)")
                : t("subagentsSyncInterval", "Subagents Sync Interval (minutes)")}
            </label>
            <input
              type="number"
              min={5}
              max={1440}
              step={5}
              disabled={!syncEnabled}
              className="w-full bg-background border rounded-xl px-3.5 py-2 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-primary/20 disabled:opacity-60"
              value={syncInterval}
              onChange={(e) => {
                const raw = parseInt(e.target.value, 10);
                const val = Number.isFinite(raw) ? Math.max(5, Math.min(1440, raw)) : 60;
                setConfig((prev) =>
                  isSkills
                    ? { ...prev, skills_sync_interval_minutes: val }
                    : { ...prev, subagents_sync_interval_minutes: val },
                );
              }}
            />
          </div>

          <div className="space-y-1.5">
            <label className="text-xs font-medium text-muted-foreground">
              {isSkills
                ? t("skillsNewBadgeHours", "New Skill Badge Duration (hours)")
                : t("subagentsNewBadgeHours", "New Subagent Badge Duration (hours)")}
            </label>
            <input
              type="number"
              min={1}
              max={720}
              step={1}
              className="w-full bg-background border rounded-xl px-3.5 py-2 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-primary/20"
              value={badgeHours}
              onChange={(e) => {
                const raw = parseInt(e.target.value, 10);
                const val = Number.isFinite(raw) ? Math.max(1, Math.min(720, raw)) : 72;
                setConfig((prev) =>
                  isSkills
                    ? { ...prev, skills_new_badge_hours: val }
                    : { ...prev, subagents_new_badge_hours: val },
                );
              }}
            />
          </div>
        </div>
      </div>

      {/* Sources Repository List */}
      <div className="bg-card border rounded-2xl p-6 shadow-sm space-y-4">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <h4 className="text-sm font-medium">
            {isSkills
              ? t("skillsSources", "Git Repository Skills Sources")
              : t("subagentsSources", "Git Repository Subagents Sources")}
          </h4>
          <div className="flex items-center gap-2">
            <input
              ref={fileInputRef}
              type="file"
              accept="application/json,.json"
              onChange={handleImport}
              className="hidden"
            />
            <button
              type="button"
              onClick={() => fileInputRef.current?.click()}
              className="inline-flex items-center gap-1.5 px-2.5 py-1.5 rounded-lg border bg-background hover:bg-muted text-xs font-medium"
            >
              <Upload className="w-3.5 h-3.5" />
              <span>{t("import", "Import")}</span>
            </button>
            <button
              type="button"
              onClick={handleExport}
              className="inline-flex items-center gap-1.5 px-2.5 py-1.5 rounded-lg border bg-background hover:bg-muted text-xs font-medium"
            >
              <Download className="w-3.5 h-3.5" />
              <span>{t("export", "Export")}</span>
            </button>
            <button
              type="button"
              onClick={() => {
                setShowAddModal(true);
                setValidation({});
              }}
              className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-primary text-primary-foreground text-xs font-medium hover:bg-primary/90 shadow-sm"
            >
              <Plus className="w-3.5 h-3.5" />
              <span>{t("addSource", "Add Source")}</span>
            </button>
          </div>
        </div>

        {sources.length === 0 ? (
          <div className="rounded-xl border border-dashed p-6 text-center text-xs text-muted-foreground bg-muted/10">
            {isSkills
              ? t(
                  "noSkillSources",
                  "No Git repository source configured yet. Add one above to enable catalog sync.",
                )
              : t(
                  "noSubagentSources",
                  "No Git repository source configured yet. Add one above to enable catalog sync.",
                )}
          </div>
        ) : (
          <div className="space-y-3">
            {sources.map((source) => {
              const syncInfo = syncSourceMap.get(source.id);
              const syncFailed =
                !!syncInfo?.last_error ||
                !!syncInfo?.last_status?.includes("error");
              const syncSucceeded = !syncFailed && !!syncInfo?.last_synced_at;
              const diagnosing = !isSkills && subagentSourceDiagnosing[source.id];
              const diagResult = !isSkills && subagentSourceDiagnostics[source.id];

              return (
                <div
                  key={source.id}
                  className={`rounded-xl border p-4 transition-all ${
                    source.enabled ? "bg-background" : "bg-muted/30 opacity-70"
                  }`}
                >
                  <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-3">
                    <div className="space-y-1 min-w-0 flex-1">
                      <div className="flex items-center gap-2">
                        <span className="text-sm font-semibold truncate">
                          {source.name || source.id}
                        </span>
                        <span className="text-xs px-2 py-0.5 rounded-md bg-muted text-muted-foreground font-mono">
                          {source.id}
                        </span>
                        {source.enabled ? (
                          <span className="w-2 h-2 rounded-full bg-emerald-500" />
                        ) : (
                          <span className="w-2 h-2 rounded-full bg-gray-400" />
                        )}
                      </div>
                      <div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground font-mono">
                        <span className="truncate max-w-sm">{source.repo_url}</span>
                        <span>•</span>
                        <span>branch: {source.branch || "main"}</span>
                        <span>•</span>
                        <span>dir: {source.base_dir || "/"}</span>
                      </div>
                    </div>

                    <div className="flex items-center gap-2 shrink-0">
                      <button
                        type="button"
                        onClick={() => handleCopyRepo(source.repo_url)}
                        title={t("copyToClipboard", "Copy repository URL")}
                        className="p-1.5 text-muted-foreground hover:text-foreground rounded-lg hover:bg-muted"
                      >
                        <Copy className="w-4 h-4" />
                      </button>

                      {!isSkills && onDiagnoseSubagentSource && (
                        <button
                          type="button"
                          onClick={() => onDiagnoseSubagentSource(source.id)}
                          disabled={diagnosing || !source.id}
                          className="px-2.5 py-1 text-xs font-medium rounded-lg border bg-background hover:bg-muted inline-flex items-center gap-1.5 disabled:opacity-50"
                        >
                          <Activity
                            className={`w-3.5 h-3.5 ${diagnosing ? "animate-spin text-primary" : ""}`}
                          />
                          <span>{t("diagnose", "Diagnose")}</span>
                        </button>
                      )}

                      <button
                        type="button"
                        onClick={() =>
                          handleToggleSource(source.id, !source.enabled)
                        }
                        className={`px-2.5 py-1 text-xs font-medium rounded-lg border ${
                          source.enabled
                            ? "bg-secondary text-secondary-foreground"
                            : "bg-muted text-muted-foreground"
                        }`}
                      >
                        {source.enabled
                          ? t("enabled", "Enabled")
                          : t("disabled", "Disabled")}
                      </button>

                      <button
                        type="button"
                        onClick={() => handleDeleteSource(source.id)}
                        className="p-1.5 text-muted-foreground hover:text-destructive rounded-lg hover:bg-destructive/10"
                        title={t("delete", "Delete")}
                      >
                        <Trash2 className="w-4 h-4" />
                      </button>
                    </div>
                  </div>

                  {/* Sync status info if any */}
                  {syncInfo && (
                    <div className="mt-2.5 pt-2 border-t text-xs flex items-center gap-1.5">
                      {syncFailed ? (
                        <span className="text-destructive inline-flex items-center gap-1">
                          <AlertCircle className="w-3.5 h-3.5" />
                          {syncInfo.last_error || "Sync failed"}
                        </span>
                      ) : syncSucceeded ? (
                        <span className="text-emerald-600 inline-flex items-center gap-1">
                          <CheckCircle2 className="w-3.5 h-3.5" />
                          {t("syncSuccess", "Synced")}:{" "}
                          {new Date(syncInfo.last_synced_at!).toLocaleTimeString()}
                        </span>
                      ) : null}
                    </div>
                  )}

                  {/* Diagnostic details if any */}
                  {diagResult && (
                    <div className="mt-2.5 pt-2 border-t text-xs space-y-1 bg-muted/20 p-2.5 rounded-lg">
                      <div className="flex items-center gap-4 text-muted-foreground">
                        <span>Total: {diagResult.total_entries}</span>
                        <span className="text-emerald-600">
                          Accepted: {diagResult.accepted_entries}
                        </span>
                        <span className="text-amber-600">
                          Skipped: {diagResult.skipped_entries}
                        </span>
                      </div>
                      {diagResult.skipped_samples &&
                        diagResult.skipped_samples.length > 0 && (
                          <div className="text-[11px] text-muted-foreground font-mono space-y-0.5 pt-1">
                            {diagResult.skipped_samples.slice(0, 3).map((sample, i) => (
                              <div key={i} className="truncate">
                                • {sample.rel_path} ({sample.reason})
                              </div>
                            ))}
                          </div>
                        )}
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        )}
      </div>

      {/* Add Source Modal */}
      {showAddModal && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm p-4 animate-in fade-in duration-200">
          <div className="bg-card border rounded-2xl p-6 shadow-xl w-full max-w-lg space-y-4">
            <div className="flex items-center justify-between border-b pb-3">
              <h3 className="text-base font-semibold">
                {isSkills
                  ? t("addSkillSource", "Add Skills Source")
                  : t("addSubagentSource", "Add Subagents Source")}
              </h3>
              <button
                type="button"
                onClick={() => setShowAddModal(false)}
                className="text-muted-foreground hover:text-foreground"
              >
                <X className="w-5 h-5" />
              </button>
            </div>

            <div className="space-y-3 text-sm">
              <div className="space-y-1">
                <label className="text-xs font-medium text-muted-foreground">
                  {t("sourceId", "Source Identifier (ID)")} *
                </label>
                <input
                  type="text"
                  placeholder="e.g. team-curated"
                  value={newSource.id}
                  onChange={(e) =>
                    setNewSource((prev) => ({ ...prev, id: e.target.value }))
                  }
                  className="w-full bg-background border rounded-xl px-3.5 py-2 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-primary/20"
                />
                {validation.id && (
                  <p className="text-xs text-destructive">{validation.id}</p>
                )}
              </div>

              <div className="space-y-1">
                <label className="text-xs font-medium text-muted-foreground">
                  {t("sourceName", "Display Name")}
                </label>
                <input
                  type="text"
                  placeholder="e.g. Official Curated"
                  value={newSource.name}
                  onChange={(e) =>
                    setNewSource((prev) => ({ ...prev, name: e.target.value }))
                  }
                  className="w-full bg-background border rounded-xl px-3.5 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-primary/20"
                />
              </div>

              <div className="space-y-1">
                <label className="text-xs font-medium text-muted-foreground">
                  {t("gitRepoUrl", "Git Repository URL")} *
                </label>
                <input
                  type="text"
                  placeholder="https://github.com/..."
                  value={newSource.repo_url}
                  onChange={(e) =>
                    setNewSource((prev) => ({
                      ...prev,
                      repo_url: e.target.value,
                    }))
                  }
                  className="w-full bg-background border rounded-xl px-3.5 py-2 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-primary/20"
                />
                {validation.repo_url && (
                  <p className="text-xs text-destructive">
                    {validation.repo_url}
                  </p>
                )}
              </div>

              <div className="grid grid-cols-2 gap-3">
                <div className="space-y-1">
                  <label className="text-xs font-medium text-muted-foreground">
                    {t("gitBranch", "Branch")}
                  </label>
                  <input
                    type="text"
                    placeholder="main"
                    value={newSource.branch}
                    onChange={(e) =>
                      setNewSource((prev) => ({
                        ...prev,
                        branch: e.target.value,
                      }))
                    }
                    className="w-full bg-background border rounded-xl px-3.5 py-2 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-primary/20"
                  />
                </div>
                <div className="space-y-1">
                  <label className="text-xs font-medium text-muted-foreground">
                    {t("gitBaseDir", "Base Directory")}
                  </label>
                  <input
                    type="text"
                    placeholder="/"
                    value={newSource.base_dir}
                    onChange={(e) =>
                      setNewSource((prev) => ({
                        ...prev,
                        base_dir: e.target.value,
                      }))
                    }
                    className="w-full bg-background border rounded-xl px-3.5 py-2 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-primary/20"
                  />
                </div>
              </div>

              <div className="space-y-1.5 pt-1">
                <label className="text-xs font-medium text-muted-foreground">
                  {t("targetAiModels", "Target AI Models")}
                </label>
                <div className="flex flex-wrap gap-2">
                  {skillModelOptions.map((opt) => {
                    const active = (newSource.default_models || []).includes(
                      opt.id,
                    );
                    return (
                      <button
                        key={opt.id}
                        type="button"
                        onClick={() => toggleModelForNewSource(opt.id)}
                        className={`px-2.5 py-1 rounded-lg text-xs font-medium border transition-all ${
                          active
                            ? "bg-primary text-primary-foreground border-primary"
                            : "bg-muted text-muted-foreground hover:text-foreground"
                        }`}
                      >
                        {opt.label}
                      </button>
                    );
                  })}
                </div>
              </div>
            </div>

            <div className="flex items-center justify-end gap-2 pt-3 border-t">
              <button
                type="button"
                onClick={() => setShowAddModal(false)}
                className="px-4 py-2 rounded-xl border text-sm font-medium hover:bg-muted transition-all"
              >
                {t("cancel", "Cancel")}
              </button>
              <button
                type="button"
                onClick={handleAddSource}
                className="px-4 py-2 rounded-xl bg-primary text-primary-foreground text-sm font-medium hover:bg-primary/90 transition-all shadow-sm"
              >
                {t("confirm", "Add Source")}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
