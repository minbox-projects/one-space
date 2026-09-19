import { useEffect, useState, type KeyboardEvent } from "react";
import { useTranslation } from "react-i18next";
import { Eye, EyeOff, Server } from "lucide-react";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import type {
  CreateProviderFromTemplateRequest,
  GatewayProviderTemplate,
  GatewayUpstreamProtocol,
} from "@/lib/apiGateway";

type TemplateCreateDialogProps = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  template: GatewayProviderTemplate | null;
  busy: boolean;
  onConfirm: (request: CreateProviderFromTemplateRequest) => Promise<boolean>;
};

export function TemplateCreateDialog({
  open,
  onOpenChange,
  template,
  busy,
  onConfirm,
}: TemplateCreateDialogProps) {
  const { t } = useTranslation();
  const [name, setName] = useState("");
  const [baseUrl, setBaseUrl] = useState("");
  const [protocol, setProtocol] =
    useState<GatewayUpstreamProtocol>("chat_completions");
  const [apiKey, setApiKey] = useState("");
  const [revealApiKey, setRevealApiKey] = useState(false);
  const [apiKeyError, setApiKeyError] = useState(false);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    if (!open) return;
    setName(template?.name ?? "");
    setBaseUrl(template?.base_url ?? "");
    setProtocol(template?.protocol ?? "chat_completions");
    setApiKey("");
    setRevealApiKey(false);
    setApiKeyError(false);
    setSaving(false);
  }, [open, template]);

  if (!template) return null;

  const disabled = busy || saving;

  const handleSubmit = async () => {
    if (disabled) return;
    const trimmedKey = apiKey.trim();
    if (trimmedKey === "") {
      setApiKeyError(true);
      return;
    }
    setSaving(true);
    try {
      const created = await onConfirm({
        templateId: template.id,
        name: name.trim(),
        baseUrl: baseUrl.trim(),
        protocol,
        apiKey: trimmedKey,
      });
      if (created) onOpenChange(false);
    } finally {
      setSaving(false);
    }
  };

  const handleKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "Enter" && !disabled) {
      event.preventDefault();
      void handleSubmit();
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        className="w-full sm:max-w-xl overflow-hidden flex flex-col sm:rounded-2xl p-0 gap-0"
        data-testid="api-gateway-template-create-dialog"
      >
        <DialogHeader className="pl-6 pr-14 py-4 border-b bg-card/80 backdrop-blur-sm shrink-0">
          <div className="flex items-center gap-3 min-w-0">
            <div className="flex h-9 w-9 shrink-0 items-center justify-center rounded-xl border border-primary/20 bg-primary/10 text-primary shadow-2xs">
              <Server className="h-4.5 w-4.5" />
            </div>
            <div className="min-w-0">
              <div className="flex items-center gap-2">
                <DialogTitle className="truncate text-base font-semibold leading-5 text-foreground">
                  {t("apiGatewayTemplateCreateTitle", "Add provider from template")}
                </DialogTitle>
                <span className="rounded-full bg-primary/10 px-2 py-0.5 text-[11px] font-medium text-primary">
                  {template.name}
                </span>
              </div>
              <DialogDescription className="text-xs text-muted-foreground mt-0.5 line-clamp-1">
                {t("apiGatewayTemplateCreateDesc", {
                  name: template.name,
                  defaultValue:
                    "Create an upstream provider from {{name}}. Every enabled template model is added as a mapping.",
                })}
              </DialogDescription>
            </div>
          </div>
        </DialogHeader>

        <div className="space-y-4 p-6 overflow-y-auto">
          {/* Name 独占整行 */}
          <div className="field full-span">
            <label className="required">
              {t("apiGatewayTemplateNameLabel", "Name")}
            </label>
            <input
              type="text"
              value={name}
              onChange={(event) => setName(event.target.value)}
              aria-label={t("apiGatewayTemplateNameLabel", "Name")}
              disabled={disabled}
            />
          </div>

          {/* Protocol 与映射数量预览同行 */}
          <div className="field-grid col-2 mb-0">
            <div className="field">
              <label>{t("apiGatewayTemplateProtocolLabel", "API protocol")}</label>
              <select
                value={protocol}
                onChange={(event) =>
                  setProtocol(event.target.value as GatewayUpstreamProtocol)
                }
                aria-label={t("apiGatewayTemplateProtocolLabel", "API protocol")}
                disabled={disabled}
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
              <label>{t("models", "Models")}</label>
              <div className="flex h-[38px] items-center rounded-lg border border-border bg-muted/40 px-3 text-xs text-muted-foreground">
                {t("apiGatewayTemplateModelsCount", {
                  count: template.models.length,
                  defaultValue: "{{count}} models",
                })}
              </div>
            </div>
          </div>

          {/* Base URL 独占整行 */}
          <div className="field full-span">
            <label>{t("apiGatewayTemplateBaseUrlLabel", "API base URL")}</label>
            <input
              type="text"
              value={baseUrl}
              onChange={(event) => setBaseUrl(event.target.value)}
              aria-label={t("apiGatewayTemplateBaseUrlLabel", "API base URL")}
              disabled={disabled}
            />
          </div>

          {/* API Key 独占整行且必填 */}
          <div className="field full-span">
            <label className="required">
              {t("apiGatewayTemplateApiKeyLabel", "API key")}
            </label>
            <div className="relative">
              <input
                type={revealApiKey ? "text" : "password"}
                value={apiKey}
                data-testid="api-gateway-template-api-key"
                onChange={(event) => {
                  setApiKey(event.target.value);
                  if (apiKeyError && event.target.value.trim() !== "") {
                    setApiKeyError(false);
                  }
                }}
                onKeyDown={handleKeyDown}
                aria-label={t("apiGatewayTemplateApiKeyLabel", "API key")}
                aria-invalid={apiKeyError}
                disabled={disabled}
                className={
                  apiKeyError
                    ? "border-destructive focus:border-destructive focus:ring-destructive/40"
                    : undefined
                }
              />
              <button
                type="button"
                onClick={() => setRevealApiKey((prev) => !prev)}
                aria-label={
                  revealApiKey
                    ? t("apiGatewayHideSecret", "Hide secret")
                    : t("apiGatewayShowSecret", "Show secret")
                }
                className="absolute right-1.5 top-1/2 inline-flex h-6 w-6 -translate-y-1/2 items-center justify-center rounded-md text-muted-foreground transition hover:bg-muted hover:text-foreground"
              >
                {revealApiKey ? (
                  <EyeOff className="h-3.5 w-3.5" />
                ) : (
                  <Eye className="h-3.5 w-3.5" />
                )}
              </button>
            </div>
            {apiKeyError ? (
              <p
                data-testid="api-gateway-template-api-key-error"
                className="mt-1 text-[11px] text-destructive"
              >
                {t("apiGatewayTemplateApiKeyRequired", "API key is required.")}
              </p>
            ) : null}
          </div>
        </div>

        <DialogFooter className="px-6 py-4 border-t bg-card/80 backdrop-blur-sm shrink-0 flex flex-row items-center justify-end gap-2">
          <button
            type="button"
            onClick={() => onOpenChange(false)}
            disabled={disabled}
            className="acc-panel-btn"
          >
            {t("cancel", "Cancel")}
          </button>
          <button
            type="button"
            data-testid="api-gateway-template-create-submit"
            onClick={() => void handleSubmit()}
            disabled={disabled}
            className="acc-panel-btn primary"
          >
            {t("apiGatewayTemplateCreateSubmit", "Create provider")}
          </button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
