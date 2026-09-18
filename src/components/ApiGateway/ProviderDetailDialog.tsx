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
import { Switch } from "@/components/ui/switch";
import {
  type GatewayModelMapping,
  type GatewayUpstreamProtocol,
  type GatewayUpstreamProvider,
} from "@/lib/apiGateway";

type ProviderDetailDialogProps = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  provider: GatewayUpstreamProvider | null;
  busy: boolean;
  onSave: (provider: GatewayUpstreamProvider) => void;
  onDelete?: (providerId: string) => void;
};

const mappingInputClass =
  "h-[38px] rounded-lg border border-border bg-background px-3 text-sm text-foreground outline-none transition-all focus:border-primary focus:ring-2 focus:ring-primary/50";

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
  const [protocol, setProtocol] = useState<GatewayUpstreamProtocol>("chat_completions");
  const [mappings, setMappings] = useState<GatewayModelMapping[]>([]);
  const [revealApiKey, setRevealApiKey] = useState(false);

  useEffect(() => {
    if (!provider) {
      setName("");
      setBaseUrl("");
      setApiKey("");
      setDefaultModel("");
      setProtocol("chat_completions");
      setMappings([]);
      setRevealApiKey(false);
      return;
    }
    setName(provider.name);
    setBaseUrl(provider.base_url);
    setApiKey(provider.api_key);
    setDefaultModel(provider.default_model ?? "");
    setProtocol(provider.protocol ?? "chat_completions");
    setMappings(provider.mappings ?? []);
    setRevealApiKey(false);
  }, [provider, open]);

  if (!provider) return null;

  const isEditing = Boolean(provider.id);

  const updateMapping = (index: number, patch: Partial<GatewayModelMapping>) => {
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
        enabled: mapping.enabled !== false,
        display_name: mapping.display_name?.trim() ? mapping.display_name.trim() : undefined,
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
        className="max-h-[90vh] w-full sm:max-w-6xl overflow-y-auto sm:rounded-xl p-5"
        data-testid="api-gateway-provider-detail"
      >
        <DialogHeader className="space-y-1">
          <DialogTitle className="text-base font-semibold">
            {isEditing
              ? t("apiGatewayEditProvider", "Edit provider")
              : t("apiGatewayNewProvider", "New provider")}
          </DialogTitle>
          <DialogDescription className="text-xs text-muted-foreground">
            {t(
              "apiGatewayProviderDialogDesc",
              "Configure upstream provider credentials, endpoint protocol, and model routing mappings.",
            )}
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-4 py-2">
          {/* 基础配置两列网格（使用 AI 终端服务商统一的标准 field-grid 和 field） */}
          <div className="field-grid col-2 mb-0">
            {/* 第 1 行：名称独占一行 */}
            <div className="field full-span">
              <label className="required">{t("apiGatewayName", "Name")}</label>
              <input
                type="text"
                value={name}
                onChange={(event) => setName(event.target.value)}
                placeholder="e.g. DeepSeek / OpenAI"
                aria-label={t("apiGatewayName", "Name")}
              />
            </div>

            {/* 第 2 行：接口协议与默认模型并排 */}
            <div className="field">
              <label className="required">{t("apiGatewayProtocol", "API protocol")}</label>
              <select
                value={protocol}
                onChange={(event) =>
                  setProtocol(event.target.value as GatewayUpstreamProtocol)
                }
                aria-label={t("apiGatewayProtocol", "API protocol")}
              >
                <option value="chat_completions">
                  {t("apiGatewayProtocolChat", "Chat Completions (/chat/completions)")}
                </option>
                <option value="responses">
                  {t("apiGatewayProtocolResponses", "Responses (/responses)")}
                </option>
              </select>
            </div>

            <div className="field">
              <label>{t("apiGatewayDefaultModel", "Default model")}</label>
              <input
                type="text"
                value={defaultModel}
                onChange={(event) => setDefaultModel(event.target.value)}
                placeholder="e.g. gpt-4o / deepseek-chat"
                aria-label={t("apiGatewayDefaultModel", "Default model")}
                className="font-mono"
              />
            </div>

            {/* 第 3 行：API Base URL 独占一行 */}
            <div className="field full-span">
              <label className="required">{t("apiGatewayBaseUrl", "API base URL")}</label>
              <input
                type="text"
                value={baseUrl}
                onChange={(event) => setBaseUrl(event.target.value)}
                placeholder="https://api.openai.com"
                aria-label={t("apiGatewayBaseUrl", "API base URL")}
                className="font-mono"
              />
            </div>

            {/* 第 4 行：API Key 独占一行 */}
            <div className="field full-span">
              <label>{t("apiGatewayApiKey", "API key")}</label>
              <div className="relative">
                <input
                  type={revealApiKey ? "text" : "password"}
                  value={apiKey}
                  onChange={(event) => setApiKey(event.target.value)}
                  placeholder="sk-..."
                  aria-label={t("apiGatewayApiKey", "API key")}
                  className="pr-10 font-mono"
                />
                <button
                  type="button"
                  onClick={() => setRevealApiKey((prev) => !prev)}
                  aria-label={
                    revealApiKey
                      ? t("apiGatewayHideSecret", "Hide secret")
                      : t("apiGatewayShowSecret", "Show secret")
                  }
                  className="absolute right-1 top-1/2 inline-flex h-8 w-8 -translate-y-1/2 items-center justify-center rounded-md text-muted-foreground transition hover:bg-muted hover:text-foreground"
                >
                  {revealApiKey ? <EyeOff className="h-4 w-4" /> : <Eye className="h-4 w-4" />}
                </button>
              </div>
            </div>
          </div>

          {/* 模型映射列表（宽幅舒展设计，输入框尺寸与标准 field 保持一致） */}
          <div className="space-y-2 rounded-xl border bg-muted/20 p-3.5">
            <div className="flex items-center justify-between gap-3">
              <div>
                <div className="text-xs font-semibold text-foreground">
                  {t("apiGatewayModelMappings", "Model mappings")}
                </div>
                <p className="text-[11px] text-muted-foreground">
                  {t(
                    "apiGatewayModelMappingsDesc",
                    "Map local request model names to upstream models. Optionally set a display name shown in the gateway for each model.",
                  )}
                </p>
              </div>
              <button
                type="button"
                onClick={() =>
                  setMappings((prev) => [
                    ...prev,
                    {
                      local_model: "",
                      upstream_model: "",
                      display_name: "",
                      protocol: null,
                      enabled: true,
                    },
                  ])
                }
                className="inline-flex h-7 items-center gap-1.5 rounded-md border bg-background px-2 text-xs font-medium shadow-sm transition hover:bg-muted"
              >
                <Plus className="h-3 w-3" />
                {t("apiGatewayAddMapping", "Add mapping")}
              </button>
            </div>

            {mappings.length === 0 ? (
              <p className="rounded-lg border border-dashed bg-background/50 px-3 py-3 text-center text-xs text-muted-foreground">
                {t("apiGatewayNoMappings", "No model mappings configured.")}
              </p>
            ) : (
              <div className="overflow-x-auto">
                <ul className="space-y-2">
                {mappings.map((mapping, index) => (
                  <li
                    key={index}
                    data-disabled={mapping.enabled === false ? "true" : undefined}
                    className={`flex items-center gap-2 ${mapping.enabled === false ? "opacity-60" : ""}`}
                  >
                    <Switch
                      aria-label={t("apiGatewayToggleMappingAria", {
                        index: index + 1,
                        defaultValue: `Enable mapping ${index + 1}`,
                      })}
                      checked={mapping.enabled !== false}
                      onCheckedChange={(checked) =>
                        updateMapping(index, { enabled: checked })
                      }
                    />
                    <input
                      type="text"
                      value={mapping.local_model}
                      onChange={(event) =>
                        updateMapping(index, { local_model: event.target.value })
                      }
                      placeholder={t("apiGatewayLocalModelPlaceholder", "local model")}
                      aria-label={t("apiGatewayLocalModelAria", {
                        index: index + 1,
                        defaultValue: `Local model ${index + 1}`,
                      })}
                      className={`${mappingInputClass} min-w-[120px] flex-1 font-mono`}
                    />
                    <span aria-hidden="true" className="shrink-0 text-muted-foreground font-semibold text-sm">
                      →
                    </span>
                    <input
                      type="text"
                      value={mapping.upstream_model}
                      onChange={(event) =>
                        updateMapping(index, { upstream_model: event.target.value })
                      }
                      placeholder={t("apiGatewayUpstreamModelPlaceholder", "upstream model")}
                      aria-label={t("apiGatewayUpstreamModelAria", {
                        index: index + 1,
                        defaultValue: `Upstream model ${index + 1}`,
                      })}
                      className={`${mappingInputClass} min-w-[120px] flex-1 font-mono`}
                    />
                    <input
                      type="text"
                      value={mapping.display_name ?? ""}
                      onChange={(event) =>
                        updateMapping(index, { display_name: event.target.value })
                      }
                      placeholder={t("apiGatewayLocalModelNamePlaceholder", "display name")}
                      aria-label={t("apiGatewayLocalModelNameAria", {
                        index: index + 1,
                        defaultValue: `Local model name ${index + 1}`,
                      })}
                      className={`${mappingInputClass} min-w-[120px] flex-1`}
                    />
                    <select
                      value={mapping.protocol ?? ""}
                      onChange={(event) =>
                        updateMapping(index, {
                          protocol:
                            event.target.value === ""
                              ? null
                              : (event.target.value as GatewayUpstreamProtocol),
                        })
                      }
                      aria-label={t("apiGatewayMappingProtocolAria", {
                        index: index + 1,
                        defaultValue: `Mapping protocol ${index + 1}`,
                      })}
                      className={`${mappingInputClass} min-w-[180px] shrink-0`}
                    >
                      <option value="">
                        {t("apiGatewayProtocolInherit", "Inherit from provider")}
                      </option>
                      <option value="chat_completions">
                        {t("apiGatewayProtocolChat", "Chat Completions (/chat/completions)")}
                      </option>
                      <option value="responses">
                        {t("apiGatewayProtocolResponses", "Responses (/responses)")}
                      </option>
                    </select>
                    <button
                      type="button"
                      onClick={() =>
                        setMappings((prev) => prev.filter((_, entryIndex) => entryIndex !== index))
                      }
                      aria-label={t("apiGatewayRemoveMappingAria", {
                        index: index + 1,
                        defaultValue: `Remove mapping ${index + 1}`,
                      })}
                      className="inline-flex h-[38px] w-[38px] shrink-0 items-center justify-center rounded-lg text-muted-foreground transition hover:bg-destructive/10 hover:text-destructive"
                    >
                      <Trash2 className="h-4 w-4" />
                    </button>
                  </li>
                ))}
                </ul>
              </div>
            )}
          </div>
        </div>

        <DialogFooter className="flex flex-row items-center justify-between gap-2 pt-2 sm:justify-between">
          <div>
            {isEditing && onDelete ? (
              <button
                type="button"
                onClick={handleDelete}
                disabled={busy}
                className="acc-panel-btn danger"
              >
                <Trash2 />
                {t("apiGatewayDelete", "Delete")}
              </button>
            ) : null}
          </div>
          <div className="flex items-center gap-2">
            <button
              type="button"
              onClick={() => onOpenChange(false)}
              disabled={busy}
              className="acc-panel-btn"
            >
              {t("cancel", "Cancel")}
            </button>
            <button
              type="button"
              onClick={handleSave}
              disabled={busy || !name.trim() || !baseUrl.trim()}
              className="acc-panel-btn primary"
            >
              {t("apiGatewaySave", "Save")}
            </button>
          </div>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
