import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Eye, EyeOff, Plus, Trash2 } from "lucide-react";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  resolveMappingPreview,
  type FusionModelMapping,
  type FusionUpstreamProtocol,
  type FusionUpstreamProvider,
} from "@/lib/apiFusion";

type ProviderDetailDialogProps = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  provider: FusionUpstreamProvider | null;
  busy: boolean;
  onSave: (provider: FusionUpstreamProvider) => void;
  onDelete?: (providerId: string) => void;
};

const inputClass =
  "h-10 w-full rounded-md border border-input bg-background px-3 text-sm focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring";
const labelClass = "text-[11px] font-medium uppercase tracking-wider text-muted-foreground";

function endpointPath(protocol: FusionUpstreamProtocol): string {
  return protocol === "responses" ? "/responses" : "/chat/completions";
}

export function ProviderDetailDialog({
  open,
  onOpenChange,
  provider,
  busy,
  onSave,
  onDelete,
}: ProviderDetailDialogProps) {
  const { t } = useTranslation();

  const [name, setName] = useState("");
  const [baseUrl, setBaseUrl] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [defaultModel, setDefaultModel] = useState("");
  const [protocol, setProtocol] = useState<FusionUpstreamProtocol>("chat_completions");
  const [mappings, setMappings] = useState<FusionModelMapping[]>([]);
  const [revealApiKey, setRevealApiKey] = useState(false);
  const [previewModel, setPreviewModel] = useState("");

  useEffect(() => {
    if (!provider) {
      setName("");
      setBaseUrl("");
      setApiKey("");
      setDefaultModel("");
      setProtocol("chat_completions");
      setMappings([]);
      setRevealApiKey(false);
      setPreviewModel("");
      return;
    }
    setName(provider.name);
    setBaseUrl(provider.base_url);
    setApiKey(provider.api_key);
    setDefaultModel(provider.default_model ?? "");
    setProtocol(provider.protocol ?? "chat_completions");
    setMappings(provider.mappings ?? []);
    setRevealApiKey(false);
    setPreviewModel(provider.mappings[0]?.local_model ?? "");
  }, [provider, open]);

  if (!provider) return null;

  const isEditing = Boolean(provider.id);

  const preview = resolveMappingPreview(
    {
      protocol,
      mappings,
      default_model: defaultModel.trim() ? defaultModel.trim() : null,
    },
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
      protocol,
      mappings: mappings.map((mapping) => ({
        ...mapping,
        protocol: mapping.protocol ? mapping.protocol : undefined,
      })),
    });
    onOpenChange(false);
  };

  const handleDelete = () => {
    if (!provider.id || !onDelete) return;
    onDelete(provider.id);
    onOpenChange(false);
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        className="max-h-[90vh] max-w-2xl overflow-y-auto sm:rounded-2xl"
        data-testid="api-fusion-provider-detail"
      >
        <DialogHeader>
          <DialogTitle>
            {isEditing
              ? t("apiFusionEditProvider", "Edit provider")
              : t("apiFusionNewProvider", "New provider")}
          </DialogTitle>
          <DialogDescription>
            {t(
              "apiFusionProviderDialogDesc",
              "Configure upstream provider credentials, endpoint protocol, and model routing mappings.",
            )}
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-5 py-2">
          {/* 基础配置两列网格 */}
          <div className="grid gap-4 sm:grid-cols-2">
            <label className="space-y-1.5 sm:col-span-1">
              <span className={labelClass}>{t("apiFusionName", "Name")}</span>
              <input
                type="text"
                value={name}
                onChange={(event) => setName(event.target.value)}
                placeholder="e.g. DeepSeek / OpenAI"
                aria-label={t("apiFusionName", "Name")}
                className={inputClass}
              />
            </label>

            <label className="space-y-1.5 sm:col-span-1">
              <span className={labelClass}>{t("apiFusionProtocol", "API protocol")}</span>
              <select
                value={protocol}
                onChange={(event) =>
                  setProtocol(event.target.value as FusionUpstreamProtocol)
                }
                aria-label={t("apiFusionProtocol", "API protocol")}
                className={inputClass}
              >
                <option value="chat_completions">
                  {t("apiFusionProtocolChat", "Chat Completions (/chat/completions)")}
                </option>
                <option value="responses">
                  {t("apiFusionProtocolResponses", "Responses (/responses)")}
                </option>
              </select>
            </label>

            <label className="space-y-1.5 sm:col-span-2">
              <span className={labelClass}>{t("apiFusionBaseUrl", "API base URL")}</span>
              <input
                type="text"
                value={baseUrl}
                onChange={(event) => setBaseUrl(event.target.value)}
                placeholder="https://api.openai.com"
                aria-label={t("apiFusionBaseUrl", "API base URL")}
                className={`${inputClass} font-mono`}
              />
            </label>

            <label className="space-y-1.5 sm:col-span-1">
              <span className={labelClass}>{t("apiFusionApiKey", "API key")}</span>
              <div className="relative">
                <input
                  type={revealApiKey ? "text" : "password"}
                  value={apiKey}
                  onChange={(event) => setApiKey(event.target.value)}
                  placeholder="sk-..."
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
                  className="absolute right-1 top-1 inline-flex h-8 w-8 items-center justify-center rounded-md text-muted-foreground transition hover:bg-muted"
                >
                  {revealApiKey ? <EyeOff className="h-4 w-4" /> : <Eye className="h-4 w-4" />}
                </button>
              </div>
            </label>

            <label className="space-y-1.5 sm:col-span-1">
              <span className={labelClass}>{t("apiFusionDefaultModel", "Default model")}</span>
              <input
                type="text"
                value={defaultModel}
                onChange={(event) => setDefaultModel(event.target.value)}
                placeholder="e.g. gpt-4o / deepseek-chat"
                aria-label={t("apiFusionDefaultModel", "Default model")}
                className={`${inputClass} font-mono`}
              />
            </label>
          </div>

          {/* 模型映射列表 */}
          <div className="space-y-2.5 rounded-xl border bg-muted/20 p-4">
            <div className="flex items-center justify-between gap-3">
              <div>
                <span className={labelClass}>{t("apiFusionModelMappings", "Model mappings")}</span>
                <p className="text-xs text-muted-foreground">
                  {t(
                    "apiFusionModelMappingsDesc",
                    "Map local request model name to the upstream model name.",
                  )}
                </p>
              </div>
              <button
                type="button"
                onClick={() =>
                  setMappings((prev) => [
                    ...prev,
                    { local_model: "", upstream_model: "", protocol: null },
                  ])
                }
                className="inline-flex items-center gap-1.5 rounded-md border bg-background px-2.5 py-1 text-xs font-medium shadow-sm transition hover:bg-muted"
              >
                <Plus className="h-3.5 w-3.5" />
                {t("apiFusionAddMapping", "Add mapping")}
              </button>
            </div>

            {mappings.length === 0 ? (
              <p className="rounded-lg border border-dashed bg-background/50 px-3 py-4 text-center text-xs text-muted-foreground">
                {t("apiFusionNoMappings", "No model mappings configured.")}
              </p>
            ) : (
              <ul className="space-y-2">
                {mappings.map((mapping, index) => (
                  <li key={index} className="flex flex-wrap items-center gap-2">
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
                      className={`${inputClass} min-w-[120px] flex-1 font-mono`}
                    />
                    <span aria-hidden="true" className="shrink-0 text-muted-foreground font-semibold">
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
                      className={`${inputClass} min-w-[120px] flex-1 font-mono`}
                    />
                    <select
                      value={mapping.protocol ?? ""}
                      onChange={(event) =>
                        updateMapping(index, {
                          protocol:
                            event.target.value === ""
                              ? null
                              : (event.target.value as FusionUpstreamProtocol),
                        })
                      }
                      aria-label={t("apiFusionMappingProtocolAria", {
                        index: index + 1,
                        defaultValue: `Mapping protocol ${index + 1}`,
                      })}
                      className="h-10 shrink-0 rounded-md border border-input bg-background px-2 text-sm focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
                    >
                      <option value="">
                        {t("apiFusionProtocolInherit", "Inherit from provider")}
                      </option>
                      <option value="chat_completions">
                        {t("apiFusionProtocolChat", "Chat Completions (/chat/completions)")}
                      </option>
                      <option value="responses">
                        {t("apiFusionProtocolResponses", "Responses (/responses)")}
                      </option>
                    </select>
                    <button
                      type="button"
                      onClick={() =>
                        setMappings((prev) => prev.filter((_, entryIndex) => entryIndex !== index))
                      }
                      aria-label={t("apiFusionRemoveMappingAria", {
                        index: index + 1,
                        defaultValue: `Remove mapping ${index + 1}`,
                      })}
                      className="inline-flex h-10 w-10 shrink-0 items-center justify-center rounded-md text-muted-foreground transition hover:bg-destructive/10 hover:text-destructive"
                    >
                      <Trash2 className="h-4 w-4" />
                    </button>
                  </li>
                ))}
              </ul>
            )}
          </div>

          {/* 实时预览器 */}
          <div className="rounded-xl border bg-muted/10 p-4">
            <div className="mb-2 text-xs font-medium text-foreground">
              {t("apiFusionTestRouting", "Test routing & mapping")}
            </div>
            <div className="grid gap-3 sm:grid-cols-3">
              <label className="space-y-1">
                <span className={labelClass}>{t("apiFusionPreviewModel", "Preview model")}</span>
                <input
                  type="text"
                  value={previewModel}
                  onChange={(event) => setPreviewModel(event.target.value)}
                  placeholder="Enter local model name"
                  aria-label={t("apiFusionPreviewModel", "Preview model")}
                  className={`${inputClass} font-mono`}
                />
              </label>
              <div className="space-y-1">
                <span className={labelClass}>{t("apiFusionPreviewResult", "Resolved upstream model")}</span>
                <div
                  className={`flex h-10 items-center rounded-md border bg-background px-3 font-mono text-sm ${
                    preview ? "text-foreground font-medium" : "text-muted-foreground"
                  }`}
                  data-testid="api-fusion-model-preview"
                >
                  {preview?.upstreamModel ?? t("apiFusionPreviewUnavailable", "Not resolvable")}
                </div>
              </div>
              <div className="space-y-1">
                <span className={labelClass}>{t("apiFusionPreviewEndpoint", "Target endpoint")}</span>
                <div
                  className={`flex h-10 items-center rounded-md border bg-background px-3 font-mono text-sm ${
                    preview ? "text-foreground font-medium" : "text-muted-foreground"
                  }`}
                  data-testid="api-fusion-endpoint-preview"
                >
                  {preview
                    ? endpointPath(preview.endpoint)
                    : t("apiFusionPreviewUnavailable", "Not resolvable")}
                </div>
              </div>
            </div>
          </div>
        </div>

        <DialogFooter className="flex flex-row items-center justify-between gap-2 pt-2 sm:justify-between">
          <div>
            {isEditing && onDelete ? (
              <button
                type="button"
                onClick={handleDelete}
                disabled={busy}
                className="inline-flex items-center gap-1.5 rounded-lg border border-destructive/30 px-3 py-2 text-xs font-medium text-destructive transition hover:bg-destructive/10 disabled:opacity-50"
              >
                <Trash2 className="h-3.5 w-3.5" />
                {t("apiFusionDelete", "Delete")}
              </button>
            ) : null}
          </div>
          <div className="flex items-center gap-2">
            <button
              type="button"
              onClick={() => onOpenChange(false)}
              disabled={busy}
              className="rounded-lg border px-4 py-2 text-sm font-medium text-muted-foreground transition hover:bg-muted disabled:opacity-50"
            >
              {t("cancel", "Cancel")}
            </button>
            <button
              type="button"
              onClick={handleSave}
              disabled={busy || !name.trim() || !baseUrl.trim()}
              className="rounded-lg bg-primary px-4 py-2 text-sm font-medium text-primary-foreground shadow transition hover:bg-primary/90 disabled:opacity-50"
            >
              {t("apiFusionSave", "Save")}
            </button>
          </div>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
