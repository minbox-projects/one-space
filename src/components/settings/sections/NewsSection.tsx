import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Newspaper, Plus, Trash2 } from "lucide-react";
import { Switch } from "@/components/ui/switch";
import { SettingCard } from "../common/SettingCard";
import { SettingRow } from "../common/SettingRow";
import {
  type StorageConfig,
  type AiNewsRssSource,
  type AiNewsRssSourceValidation,
  BUILTIN_AI_NEWS_RSS_SOURCES,
} from "../types";
import {
  isAiNewsRssSourceConfigured,
  type NewsRetentionPreset,
} from "../helpers";

export interface NewsSectionProps {
  config: StorageConfig;
  setConfig: React.Dispatch<React.SetStateAction<StorageConfig>>;
  aiNewsSyncIntervalInput: string;
  setAiNewsSyncIntervalInput: (val: string) => void;
  newsRetentionPreset: NewsRetentionPreset;
  setNewsRetentionPreset: (preset: NewsRetentionPreset) => void;
  lastAiNewsSyncText?: string;
  onShowToast?: (type: "success" | "error", text: string) => void;
}

export function NewsSection({
  config,
  setConfig,
  aiNewsSyncIntervalInput,
  setAiNewsSyncIntervalInput,
  newsRetentionPreset,
  setNewsRetentionPreset,
  lastAiNewsSyncText = "Never",
  onShowToast,
}: NewsSectionProps) {
  const { t } = useTranslation();

  const [newSource, setNewSource] = useState<AiNewsRssSource>({
    id: "",
    name: "",
    url: "",
    enabled: true,
  });
  const [newSourceValidation, setNewSourceValidation] =
    useState<AiNewsRssSourceValidation>({});

  const validateSource = (
    src: AiNewsRssSource,
    all: AiNewsRssSource[],
    excludeId?: string,
  ): AiNewsRssSourceValidation => {
    const err: AiNewsRssSourceValidation = {};
    if (!src.id.trim()) {
      err.id = t("fieldRequired", "This field is required");
    } else if (
      all.some(
        (s) => s.id.trim() === src.id.trim() && s.id.trim() !== excludeId,
      )
    ) {
      err.id = t("idAlreadyExists", "ID already exists");
    }
    if (!src.url.trim()) {
      err.url = t("fieldRequired", "This field is required");
    }
    return err;
  };

  const addBuiltinSource = (builtin: AiNewsRssSource) => {
    const current = config.ai_news_rss_sources || [];
    if (isAiNewsRssSourceConfigured(builtin, current)) return;
    setConfig((prev) => ({
      ...prev,
      ai_news_rss_sources: [...(prev.ai_news_rss_sources || []), { ...builtin }],
    }));
  };

  const updateSource = (index: number, patch: Partial<AiNewsRssSource>) => {
    setConfig((prev) => {
      const current = [...(prev.ai_news_rss_sources || [])];
      current[index] = { ...current[index], ...patch };
      return { ...prev, ai_news_rss_sources: current };
    });
  };

  const removeSource = (index: number) => {
    setConfig((prev) => {
      const current = [...(prev.ai_news_rss_sources || [])];
      current.splice(index, 1);
      return { ...prev, ai_news_rss_sources: current };
    });
  };

  const handleAddNewSource = () => {
    const current = config.ai_news_rss_sources || [];
    const errors = validateSource(newSource, current);
    setNewSourceValidation(errors);
    if (Object.keys(errors).length > 0) {
      if (onShowToast) {
        onShowToast(
          "error",
          errors.id ||
            errors.url ||
            t("pleaseCheckInputs", "Please check your inputs"),
        );
      }
      return;
    }

    setConfig((prev) => ({
      ...prev,
      ai_news_rss_sources: [
        ...(prev.ai_news_rss_sources || []),
        {
          ...newSource,
          id: newSource.id.trim(),
          name: newSource.name.trim() || newSource.id.trim(),
          url: newSource.url.trim(),
        },
      ],
    }));
    setNewSource({ id: "", name: "", url: "", enabled: true });
    setNewSourceValidation({});
  };

  const applyRetentionPreset = (preset: NewsRetentionPreset) => {
    setNewsRetentionPreset(preset);
    if (preset === "7d_200") {
      setConfig((prev) => ({
        ...prev,
        ai_news_retention_days: 7,
        ai_news_retention_max_items: 200,
      }));
    } else if (preset === "30d_500") {
      setConfig((prev) => ({
        ...prev,
        ai_news_retention_days: 30,
        ai_news_retention_max_items: 500,
      }));
    } else if (preset === "90d_1000") {
      setConfig((prev) => ({
        ...prev,
        ai_news_retention_days: 90,
        ai_news_retention_max_items: 1000,
      }));
    }
  };

  return (
    <div className="space-y-6">
      <SettingCard
        id="section-news"
        icon={Newspaper}
        title={t("newsSettingsMenu", "News")}
        description={t(
          "newsSettingsDesc",
          "Configure AI news fetching, retention, and sync behavior.",
        )}
      >
        {/* Enable AI News switch */}
        <SettingRow
          label={t("newsEnabled", "Enable AI News")}
          description={t(
            "newsEnabledDesc",
            "When enabled, OneSpace fetches latest AI news in the background.",
          )}
        >
          <Switch
            checked={!!config.ai_news_enabled}
            onCheckedChange={(checked) =>
              setConfig((prev) => ({ ...prev, ai_news_enabled: checked }))
            }
          />
        </SettingRow>

        {config.ai_news_enabled && (
          <>
            <hr className="border-border/50 animate-in fade-in" />

            {/* Fetch Interval */}
            <div className="space-y-2">
              <label className="text-sm font-medium text-muted-foreground">
                {t("newsSyncInterval", "Fetch Interval (minutes)")}
              </label>
              <input
                type="text"
                inputMode="numeric"
                pattern="[0-9]*"
                className="w-full bg-background border rounded-xl px-4 py-2.5 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-primary/20"
                value={aiNewsSyncIntervalInput}
                onChange={(e) => setAiNewsSyncIntervalInput(e.target.value)}
              />
              <p className="text-xs text-muted-foreground">
                {t("newsLastFetchedAt", "Last fetched at: {{time}}", {
                  time: lastAiNewsSyncText,
                })}
              </p>
            </div>

            <hr className="border-border/50" />

            {/* Retention Strategy */}
            <div className="space-y-3">
              <label className="text-sm font-medium text-muted-foreground">
                {t("newsRetention", "Retention Strategy")}
              </label>
              <div className="grid grid-cols-2 sm:grid-cols-4 gap-2">
                {[
                  { id: "7d_200", label: "7天 / 200条" },
                  { id: "30d_500", label: "30天 / 500条" },
                  { id: "90d_1000", label: "90天 / 1000条" },
                  { id: "custom", label: "自定义" },
                ].map((item) => (
                  <button
                    key={item.id}
                    type="button"
                    onClick={() =>
                      applyRetentionPreset(item.id as NewsRetentionPreset)
                    }
                    className={`py-2 px-3 rounded-xl border text-xs sm:text-sm font-medium transition-all ${
                      newsRetentionPreset === item.id
                        ? "bg-primary text-primary-foreground border-primary shadow-sm"
                        : "bg-background text-muted-foreground hover:text-foreground"
                    }`}
                  >
                    {item.label}
                  </button>
                ))}
              </div>

              {newsRetentionPreset === "custom" && (
                <div className="grid grid-cols-1 sm:grid-cols-2 gap-3 pt-2">
                  <div className="space-y-1">
                    <label className="text-xs text-muted-foreground">
                      {t("retentionDays", "Retention Days")}
                    </label>
                    <input
                      type="number"
                      min={1}
                      max={365}
                      value={config.ai_news_retention_days ?? 90}
                      onChange={(e) =>
                        setConfig((prev) => ({
                          ...prev,
                          ai_news_retention_days: parseInt(e.target.value) || 90,
                        }))
                      }
                      className="w-full bg-background border rounded-xl px-3.5 py-2 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-primary/20"
                    />
                  </div>
                  <div className="space-y-1">
                    <label className="text-xs text-muted-foreground">
                      {t("maxItems", "Max Items")}
                    </label>
                    <input
                      type="number"
                      min={10}
                      max={10000}
                      value={config.ai_news_retention_max_items ?? 1000}
                      onChange={(e) =>
                        setConfig((prev) => ({
                          ...prev,
                          ai_news_retention_max_items:
                            parseInt(e.target.value) || 1000,
                        }))
                      }
                      className="w-full bg-background border rounded-xl px-3.5 py-2 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-primary/20"
                    />
                  </div>
                </div>
              )}
            </div>

            <hr className="border-border/50" />

            {/* Keywords */}
            <div className="space-y-2">
              <label className="text-sm font-medium text-muted-foreground">
                {t("newsKeywords", "News Keywords")}
              </label>
              <textarea
                rows={3}
                className="w-full bg-background border rounded-xl px-4 py-2.5 text-sm focus:outline-none focus:ring-2 focus:ring-primary/20"
                placeholder={t(
                  "newsKeywordsPlaceholder",
                  "Use comma/newline separated keywords, e.g. OpenAI, Anthropic, Gemini",
                )}
                value={config.ai_news_keywords ?? ""}
                onChange={(e) =>
                  setConfig((prev) => ({
                    ...prev,
                    ai_news_keywords: e.target.value,
                  }))
                }
              />
              <p className="text-xs text-muted-foreground">
                {t(
                  "newsKeywordsDesc",
                  "Leave empty to disable keyword filtering. Otherwise RSS items are kept when any comma, semicolon, or newline separated keyword matches the title, summary, or source.",
                )}
              </p>
            </div>

            <hr className="border-border/50" />

            {/* RSS Sources */}
            <div className="space-y-4">
              <div>
                <h3 className="text-sm font-medium">
                  {t("newsRssSources", "RSS Sources")}
                </h3>
                <p className="text-xs text-muted-foreground mt-0.5">
                  {t(
                    "newsRssSourcesDesc",
                    "Configure one or more RSS feeds used for AI News.",
                  )}
                </p>
              </div>

              {/* Builtin Sources Recommendations */}
              <div className="space-y-2">
                <h4 className="text-xs font-semibold text-muted-foreground">
                  {t(
                    "newsBuiltinRecommendations",
                    "Built-in Recommendations",
                  )}
                </h4>
                <div className="grid gap-2 sm:grid-cols-2">
                  {BUILTIN_AI_NEWS_RSS_SOURCES.map((source) => {
                    const configured = isAiNewsRssSourceConfigured(
                      source,
                      config.ai_news_rss_sources || [],
                    );
                    return (
                      <div
                        key={source.id}
                        className="flex items-center justify-between gap-3 rounded-xl border bg-muted/20 px-3.5 py-2.5"
                      >
                        <div className="min-w-0">
                          <div className="truncate text-sm font-medium">
                            {source.name}
                          </div>
                          <div className="truncate text-xs font-mono text-muted-foreground">
                            {source.url}
                          </div>
                        </div>
                        <button
                          type="button"
                          disabled={configured}
                          onClick={() => addBuiltinSource(source)}
                          className="shrink-0 rounded-lg border px-3 py-1 text-xs font-medium bg-background hover:bg-muted disabled:cursor-not-allowed disabled:opacity-50"
                        >
                          {configured
                            ? t("sourceAdded", "Added")
                            : t("add", "Add")}
                        </button>
                      </div>
                    );
                  })}
                </div>
              </div>

              {/* Configured RSS Sources */}
              <div className="space-y-2 pt-2">
                <h4 className="text-xs font-semibold text-muted-foreground">
                  {t("configuredSources", "Configured Feeds")}
                </h4>
                {(config.ai_news_rss_sources || []).map((source, index) => (
                  <div
                    key={source.id || index}
                    className="grid grid-cols-[120px_1fr_auto_auto] gap-2 items-center rounded-xl border bg-background p-2.5"
                  >
                    <input
                      className="rounded-lg border bg-background px-2.5 py-1.5 text-xs font-mono focus:outline-none focus:ring-2 focus:ring-primary/20 truncate"
                      placeholder={t("rssSourceIdPlaceholder", "source-id")}
                      value={source.id}
                      onChange={(e) =>
                        updateSource(index, { id: e.target.value })
                      }
                    />
                    <input
                      className="rounded-lg border bg-background px-2.5 py-1.5 text-xs font-mono focus:outline-none focus:ring-2 focus:ring-primary/20 truncate"
                      placeholder={t(
                        "rssSourceUrlPlaceholder",
                        "https://example.com/feed.xml",
                      )}
                      value={source.url}
                      onChange={(e) =>
                        updateSource(index, { url: e.target.value })
                      }
                    />
                    <Switch
                      checked={source.enabled !== false}
                      onCheckedChange={(checked) =>
                        updateSource(index, { enabled: checked })
                      }
                    />
                    <button
                      type="button"
                      onClick={() => removeSource(index)}
                      className="p-1.5 text-muted-foreground hover:text-destructive rounded-lg hover:bg-destructive/10"
                      title={t("delete", "Delete")}
                    >
                      <Trash2 className="w-4 h-4" />
                    </button>
                  </div>
                ))}

                {/* Add new RSS feed input */}
                <div className="grid grid-cols-[120px_140px_1fr_auto] gap-2 items-center rounded-xl border border-dashed p-2.5 bg-muted/10">
                  <input
                    className={`rounded-lg border bg-background px-2.5 py-1.5 text-xs font-mono focus:outline-none focus:ring-2 focus:ring-primary/20 truncate ${
                      newSourceValidation.id ? "border-destructive focus:ring-destructive/20" : ""
                    }`}
                    placeholder={t("rssSourceIdPlaceholder", "source-id")}
                    value={newSource.id}
                    onChange={(e) => {
                      setNewSource((prev) => ({ ...prev, id: e.target.value }));
                      if (newSourceValidation.id) {
                        setNewSourceValidation((prev) => ({ ...prev, id: undefined }));
                      }
                    }}
                  />
                  <input
                    className="rounded-lg border bg-background px-2.5 py-1.5 text-xs focus:outline-none focus:ring-2 focus:ring-primary/20 truncate"
                    placeholder={t("name", "Name")}
                    value={newSource.name}
                    onChange={(e) =>
                      setNewSource((prev) => ({ ...prev, name: e.target.value }))
                    }
                  />
                  <input
                    className={`rounded-lg border bg-background px-2.5 py-1.5 text-xs font-mono focus:outline-none focus:ring-2 focus:ring-primary/20 truncate ${
                      newSourceValidation.url ? "border-destructive focus:ring-destructive/20" : ""
                    }`}
                    placeholder={t(
                      "rssSourceUrlPlaceholder",
                      "https://example.com/feed.xml",
                    )}
                    value={newSource.url}
                    onChange={(e) => {
                      setNewSource((prev) => ({ ...prev, url: e.target.value }));
                      if (newSourceValidation.url) {
                        setNewSourceValidation((prev) => ({ ...prev, url: undefined }));
                      }
                    }}
                  />
                  <button
                    type="button"
                    onClick={handleAddNewSource}
                    className="px-3 py-1.5 bg-primary text-primary-foreground rounded-lg text-xs font-medium hover:bg-primary/90 flex items-center gap-1 shadow-sm"
                  >
                    <Plus className="w-3.5 h-3.5" />
                    <span>{t("add", "Add")}</span>
                  </button>
                </div>
              </div>
            </div>
          </>
        )}
      </SettingCard>
    </div>
  );
}
