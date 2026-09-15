import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Eye, EyeOff, Plus, Trash2 } from "lucide-react";
import {
  resolveUpstreamModelPreview,
  type FusionModelMapping,
  type FusionUpstreamProvider,
} from "@/lib/apiFusion";

type UpstreamProviderDetailProps = {
  provider: FusionUpstreamProvider;
  busy: boolean;
  onSave: (provider: FusionUpstreamProvider) => void;
  onDelete: (providerId: string) => void;
};

const inputClass =
  "h-10 w-full rounded-md border border-input bg-background px-3 text-sm";
const labelClass = "text-[10px] uppercase tracking-wide text-muted-foreground";

export function UpstreamProviderDetail({
  provider,
  busy,
  onSave,
  onDelete,
}: UpstreamProviderDetailProps) {
  const { t } = useTranslation();
  const [name, setName] = useState(provider.name);
  const [baseUrl, setBaseUrl] = useState(provider.base_url);
  const [apiKey, setApiKey] = useState(provider.api_key);
  const [defaultModel, setDefaultModel] = useState(provider.default_model ?? "");
  const [mappings, setMappings] = useState<FusionModelMapping[]>(provider.mappings);
  const [revealApiKey, setRevealApiKey] = useState(false);
  const [previewModel, setPreviewModel] = useState(
    provider.mappings[0]?.local_model ?? "",
  );

  const preview = resolveUpstreamModelPreview(
    { mappings, default_model: defaultModel.trim() ? defaultModel.trim() : null },
    previewModel,
  );

  const updateMapping = (index: number, patch: Partial<FusionModelMapping>) => {
    setMappings((prev) =>
      prev.map((entry, entryIndex) =>
        entryIndex === index ? { ...entry, ...patch } : entry,
      ),
    );
  };

  const handleSave = () => {
    onSave({
      ...provider,
      name: name.trim(),
      base_url: baseUrl.trim(),
      api_key: apiKey,
      default_model: defaultModel.trim() ? defaultModel.trim() : null,
      mappings,
    });
  };

  return (
    <section className="rounded-[24px] border bg-card p-5" data-testid="api-fusion-provider-detail">
      <h3 className="text-sm font-semibold">
        {provider.id
          ? t("apiFusionEditProvider", "Edit provider")
          : t("apiFusionNewProvider", "New provider")}
      </h3>

      <div className="mt-4 grid gap-4 md:grid-cols-2">
        <label className="space-y-1">
          <span className={labelClass}>{t("apiFusionName", "Name")}</span>
          <input
            type="text"
            value={name}
            onChange={(event) => setName(event.target.value)}
            aria-label={t("apiFusionName", "Name")}
            className={inputClass}
          />
        </label>
        <label className="space-y-1">
          <span className={labelClass}>{t("apiFusionBaseUrl", "API base URL")}</span>
          <input
            type="text"
            value={baseUrl}
            onChange={(event) => setBaseUrl(event.target.value)}
            aria-label={t("apiFusionBaseUrl", "API base URL")}
            className={`${inputClass} font-mono`}
          />
        </label>
        <label className="space-y-1">
          <span className={labelClass}>{t("apiFusionApiKey", "API key")}</span>
          <div className="relative">
            <input
              type={revealApiKey ? "text" : "password"}
              value={apiKey}
              onChange={(event) => setApiKey(event.target.value)}
              aria-label={t("apiFusionApiKey", "API key")}
              className={`${inputClass} pr-10 font-mono`}
            />
            <button
              type="button"
              onClick={() => setRevealApiKey((prev) => !prev)}
              aria-label={
                revealApiKey
                  ? t("apiFusionHideSecret", "Hide secret")
                  : t("apiFusionShowSecret", "Show secret")
              }
              className="absolute right-1 top-1 inline-flex h-8 w-8 items-center justify-center rounded-md text-muted-foreground hover:bg-muted"
            >
              {revealApiKey ? <EyeOff className="h-4 w-4" /> : <Eye className="h-4 w-4" />}
            </button>
          </div>
        </label>
        <label className="space-y-1">
          <span className={labelClass}>{t("apiFusionDefaultModel", "Default model")}</span>
          <input
            type="text"
            value={defaultModel}
            onChange={(event) => setDefaultModel(event.target.value)}
            aria-label={t("apiFusionDefaultModel", "Default model")}
            className={`${inputClass} font-mono`}
          />
        </label>
      </div>

      <div className="mt-5 space-y-2">
        <div className="flex items-center justify-between gap-3">
          <span className={labelClass}>{t("apiFusionModelMappings", "Model mappings")}</span>
          <button
            type="button"
            onClick={() =>
              setMappings((prev) => [...prev, { local_model: "", upstream_model: "" }])
            }
            className="inline-flex items-center gap-1.5 rounded-md border px-2.5 py-1 text-xs transition hover:bg-muted"
          >
            <Plus className="h-3.5 w-3.5" />
            {t("apiFusionAddMapping", "Add mapping")}
          </button>
        </div>
        {mappings.length === 0 ? (
          <p className="rounded-xl border border-dashed px-3 py-3 text-xs text-muted-foreground">
            {t("apiFusionNoMappings", "No model mappings configured.")}
          </p>
        ) : (
          <ul className="space-y-2">
            {mappings.map((mapping, index) => (
              <li key={index} className="flex items-center gap-2">
                <input
                  type="text"
                  value={mapping.local_model}
                  onChange={(event) =>
                    updateMapping(index, { local_model: event.target.value })
                  }
                  placeholder={t("apiFusionLocalModelPlaceholder", "local model")}
                  aria-label={t("apiFusionLocalModelAria", {
                    index: index + 1,
                    defaultValue: `Local model ${index + 1}`,
                  })}
                  className={`${inputClass} font-mono`}
                />
                <span aria-hidden="true" className="text-muted-foreground">
                  →
                </span>
                <input
                  type="text"
                  value={mapping.upstream_model}
                  onChange={(event) =>
                    updateMapping(index, { upstream_model: event.target.value })
                  }
                  placeholder={t("apiFusionUpstreamModelPlaceholder", "upstream model")}
                  aria-label={t("apiFusionUpstreamModelAria", {
                    index: index + 1,
                    defaultValue: `Upstream model ${index + 1}`,
                  })}
                  className={`${inputClass} font-mono`}
                />
                <button
                  type="button"
                  onClick={() =>
                    setMappings((prev) => prev.filter((_, entryIndex) => entryIndex !== index))
                  }
                  aria-label={t("apiFusionRemoveMappingAria", {
                    index: index + 1,
                    defaultValue: `Remove mapping ${index + 1}`,
                  })}
                  className="inline-flex h-9 w-9 shrink-0 items-center justify-center rounded-md text-muted-foreground transition hover:bg-muted hover:text-destructive"
                >
                  <Trash2 className="h-4 w-4" />
                </button>
              </li>
            ))}
          </ul>
        )}
      </div>

      <div className="mt-5 grid gap-3 rounded-2xl border bg-muted/10 p-4 md:grid-cols-[minmax(0,1fr)_minmax(0,1fr)]">
        <label className="space-y-1">
          <span className={labelClass}>{t("apiFusionPreviewModel", "Preview model")}</span>
          <input
            type="text"
            value={previewModel}
            onChange={(event) => setPreviewModel(event.target.value)}
            aria-label={t("apiFusionPreviewModel", "Preview model")}
            className={`${inputClass} font-mono`}
          />
        </label>
        <div className="space-y-1">
          <span className={labelClass}>{t("apiFusionPreviewResult", "Resolved upstream model")}</span>
          <div
            className={`rounded-md border px-3 py-2 font-mono text-sm ${
              preview ? "text-foreground" : "text-muted-foreground"
            }`}
            data-testid="api-fusion-model-preview"
          >
            {preview ?? t("apiFusionPreviewUnavailable", "Not resolvable")}
          </div>
        </div>
      </div>

      <div className="mt-5 flex flex-wrap justify-end gap-2">
        {provider.id ? (
          <button
            type="button"
            onClick={() => onDelete(provider.id)}
            disabled={busy}
            className="inline-flex items-center gap-2 rounded-md border border-destructive/40 px-4 py-2 text-sm text-destructive transition hover:bg-destructive/10 disabled:opacity-50"
          >
            <Trash2 className="h-4 w-4" />
            {t("apiFusionDelete", "Delete")}
          </button>
        ) : null}
        <button
          type="button"
          onClick={handleSave}
          disabled={busy || !name.trim() || !baseUrl.trim()}
          className="rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground transition hover:bg-primary/90 disabled:opacity-50"
        >
          {t("apiFusionSave", "Save")}
        </button>
      </div>
    </section>
  );
}
