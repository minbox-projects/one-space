import React, { useEffect, useRef, useState } from "react";
import { Check, ChevronDown, Sparkles } from "lucide-react";
import { useTranslation } from "react-i18next";
import { cn } from "@/lib/utils";
import {
  OpenCodeIcon,
  CommandCodeIcon,
  OpenAIIcon,
  BUILTIN_PROVIDER_ICON_MAP,
  isBuiltinProviderIcon,
  resolveBuiltinProviderIcon,
  type BuiltinProviderIconKey,
} from "@/components/AiEnvironments/icons";

export interface ProviderTemplateIconOption {
  id: string;
  labelKey: string;
  fallbackLabel: string;
}

export const PROVIDER_TEMPLATE_ICON_OPTIONS: readonly ProviderTemplateIconOption[] = [
  {
    id: "opencode",
    labelKey: "aiGatewayTemplateIconOpenCode",
    fallbackLabel: "OpenCode",
  },
  {
    id: "commandcode",
    labelKey: "aiGatewayTemplateIconCommandCode",
    fallbackLabel: "CommandCode",
  },
  {
    id: "openai",
    labelKey: "aiGatewayTemplateIconOpenAI",
    fallbackLabel: "OpenAI",
  },
] as const;

export const PROVIDER_CUSTOM_ICON_OPTIONS: readonly ProviderTemplateIconOption[] = [
  { id: "openai", labelKey: "providerIconChatgpt", fallbackLabel: "OpenAI / ChatGPT" },
  { id: "builtin:claude", labelKey: "providerIconClaude", fallbackLabel: "Claude" },
  { id: "builtin:deepseek", labelKey: "providerIconDeepSeek", fallbackLabel: "DeepSeek" },
  { id: "builtin:kimi", labelKey: "providerIconKimi", fallbackLabel: "Kimi" },
  { id: "builtin:bailian", labelKey: "providerIconBailian", fallbackLabel: "通义千问 / 百炼" },
  { id: "builtin:zhipu", labelKey: "providerIconZhipu", fallbackLabel: "智谱 GLM" },
  { id: "builtin:minimax", labelKey: "providerIconMiniMax", fallbackLabel: "MiniMax" },
  { id: "builtin:baidu", labelKey: "providerIconBaidu", fallbackLabel: "百度千帆 / 文心" },
  { id: "builtin:tencent", labelKey: "providerIconTencent", fallbackLabel: "腾讯混元" },
  { id: "builtin:volcengine", labelKey: "providerIconVolcengine", fallbackLabel: "火山引擎" },
  { id: "builtin:doubao", labelKey: "providerIconDoubao", fallbackLabel: "豆包" },
  { id: "builtin:stepfun", labelKey: "providerIconStepFun", fallbackLabel: "阶跃星辰" },
  { id: "builtin:xfyun", labelKey: "providerIconXFYun", fallbackLabel: "讯飞星火" },
  { id: "builtin:sensenova", labelKey: "providerIconSenseNova", fallbackLabel: "商汤日日新" },
  { id: "builtin:lingyi", labelKey: "providerIconLingyi", fallbackLabel: "零一万物" },
  { id: "builtin:antigravity", labelKey: "providerIconAntigravity", fallbackLabel: "Antigravity" },
  { id: "opencode", labelKey: "aiGatewayTemplateIconOpenCode", fallbackLabel: "OpenCode" },
  { id: "commandcode", labelKey: "aiGatewayTemplateIconCommandCode", fallbackLabel: "CommandCode" },
] as const;

export function resolveProviderTemplateIcon(
  icon?: string | null,
  templateId?: string | null,
  templateName?: string | null,
  baseUrl?: string | null,
): string | null {
  const explicit = icon?.trim().toLowerCase();
  if (explicit) {
    if (explicit === "opencode" || explicit === "builtin:opencode") return "opencode";
    if (explicit === "commandcode" || explicit === "builtin:commandcode") return "commandcode";
    if (explicit === "openai" || explicit === "chatgpt" || explicit === "builtin:chatgpt" || explicit === "builtin:openai") return "openai";
    if (explicit in BUILTIN_PROVIDER_ICON_MAP) return explicit;
    const prefixed = `builtin:${explicit}` as BuiltinProviderIconKey;
    if (isBuiltinProviderIcon(prefixed)) return prefixed;
  }

  // 1. Detect from base_url domain keywords
  if (baseUrl) {
    const rawUrl = baseUrl.trim().toLowerCase();
    if (rawUrl.includes("commandcode")) return "commandcode";
    if (rawUrl.includes("opencode")) return "opencode";
    if (rawUrl.includes("openai.com")) return "openai";
    if (rawUrl.includes("deepseek.com")) return "builtin:deepseek";
    if (rawUrl.includes("anthropic.com")) return "builtin:claude";
    if (rawUrl.includes("moonshot.cn") || rawUrl.includes("kimi.ai")) return "builtin:kimi";
    if (rawUrl.includes("bigmodel.cn") || rawUrl.includes("zhipuai.cn")) return "builtin:zhipu";
    if (rawUrl.includes("minimax")) return "builtin:minimax";
    if (rawUrl.includes("aliyun.com") || rawUrl.includes("dashscope")) return "builtin:bailian";
    if (rawUrl.includes("volces.com") || rawUrl.includes("volcengine")) return "builtin:volcengine";
    if (rawUrl.includes("baidu.com") || rawUrl.includes("qianfan")) return "builtin:baidu";
    if (rawUrl.includes("tencent.com")) return "builtin:tencent";
    if (rawUrl.includes("stepfun")) return "builtin:stepfun";
    if (rawUrl.includes("xfyun")) return "builtin:xfyun";
    if (rawUrl.includes("sensetime")) return "builtin:sensenova";
    if (rawUrl.includes("lingyiwanwu") || rawUrl.includes("01.ai")) return "builtin:lingyi";
  }

  // 2. Direct string checks for legacy templates
  const text = `${templateId || ""} ${templateName || ""}`.toLowerCase();
  if (text.includes("commandcode") || text.includes("command code") || text.includes("command")) {
    return "commandcode";
  }
  if (text.includes("opencode")) {
    return "opencode";
  }
  if (text.includes("openai") || text.includes("gpt")) {
    return "openai";
  }

  // 3. Fallback to builtin provider keyword matching
  const builtin = resolveBuiltinProviderIcon({
    name: templateName,
    id: templateId,
  });
  if (builtin) {
    if (builtin === "builtin:opencode") return "opencode";
    if (builtin === "builtin:commandcode") return "commandcode";
    if (builtin === "builtin:chatgpt") return "openai";
    return builtin;
  }

  return null;
}

/**
 * Resolves the effective icon for an upstream provider:
 * 1. provider.icon (custom user icon)
 * 2. template?.icon (inherited from bound template)
 * 3. templateId inference
 * 4. base_url inference (e.g. api.commandcode.ai, deepseek.com, etc.)
 * 5. provider name / id inference using builtin provider keywords
 * 6. fallback (null)
 */
export function resolveEffectiveProviderIcon(
  provider?: {
    icon?: string | null;
    template_id?: string | null;
    base_url?: string | null;
    name?: string | null;
    id?: string | null;
  } | null,
  template?: {
    icon?: string | null;
    id?: string | null;
    name?: string | null;
  } | null,
): string | null {
  const custom = provider?.icon?.trim();
  if (custom) return custom;
  const inherited = template?.icon?.trim();
  if (inherited) return inherited;

  const tplId = template?.id || provider?.template_id;
  if (tplId) {
    const tplIcon = resolveProviderTemplateIcon(null, tplId, template?.name);
    if (tplIcon) return tplIcon;
  }

  const fromProvider = resolveProviderTemplateIcon(
    null,
    provider?.id,
    provider?.name,
    provider?.base_url,
  );
  if (fromProvider) return fromProvider;

  return null;
}

export interface ProviderTemplateIconProps {
  icon?: string | null;
  templateId?: string | null;
  templateName?: string | null;
  className?: string;
  fallback?: React.ReactNode;
}

export function ProviderTemplateIcon({
  icon,
  templateId,
  templateName,
  className,
  fallback,
}: ProviderTemplateIconProps) {
  const resolved = resolveProviderTemplateIcon(icon, templateId, templateName);
  const iconClasses = cn("h-5 w-5 shrink-0", className);

  if (resolved === "opencode") {
    return <OpenCodeIcon className={iconClasses} data-testid="provider-icon-opencode" />;
  }
  if (resolved === "commandcode") {
    return <CommandCodeIcon className={iconClasses} data-testid="provider-icon-commandcode" />;
  }
  if (resolved === "openai") {
    return <OpenAIIcon className={iconClasses} data-testid="provider-icon-openai" />;
  }
  if (resolved && isBuiltinProviderIcon(resolved)) {
    const Component = BUILTIN_PROVIDER_ICON_MAP[resolved as BuiltinProviderIconKey];
    return <Component className={iconClasses} />;
  }

  if (fallback !== undefined) {
    return <>{fallback}</>;
  }

  return <Sparkles className={iconClasses} data-testid="provider-icon-default" />;
}

export interface ProviderTemplateAvatarProps {
  icon?: string | null;
  templateId?: string | null;
  templateName?: string | null;
  baseUrl?: string | null;
  size?: number;
  className?: string;
  style?: React.CSSProperties;
  title?: string;
}

export function ProviderTemplateAvatar({
  icon,
  templateId,
  templateName,
  baseUrl,
  size = 36,
  className,
  style,
  title,
}: ProviderTemplateAvatarProps) {
  const resolved = resolveProviderTemplateIcon(icon, templateId, templateName, baseUrl);
  const radius = Math.max(8, Math.round(size * 0.28));
  const displayName = templateName || templateId || "";
  const fallbackChar = displayName.trim().charAt(0) || "?";

  return (
    <div
      className={cn(
        "inline-flex shrink-0 items-center justify-center overflow-hidden border text-slate-800 shadow-sm",
        className,
      )}
      style={{
        width: size,
        height: size,
        borderRadius: radius,
        userSelect: "none",
        borderColor: "rgba(203, 213, 225, 0.9)",
        background: "linear-gradient(180deg, #ffffff 0%, #f8fafc 100%)",
        boxShadow:
          "0 8px 20px rgba(15, 23, 42, 0.06), inset 0 1px 0 rgba(255,255,255,0.95)",
        ...style,
      }}
      title={title || displayName}
    >
      {resolved ? (
        <div className="flex h-[72%] w-[72%] items-center justify-center">
          <ProviderTemplateIcon
            icon={resolved}
            className="h-full w-full object-contain"
          />
        </div>
      ) : displayName ? (
        <span
          style={{
            fontSize: Math.round(size * 0.42),
            fontWeight: 700,
            lineHeight: 1,
            color: "#1f2937",
          }}
        >
          {fallbackChar}
        </span>
      ) : (
        <div className="flex h-[72%] w-[72%] items-center justify-center text-slate-500">
          <ProviderTemplateIcon className="h-full w-full object-contain" />
        </div>
      )}
    </div>
  );
}

export interface ProviderTemplateIconPickerProps {
  value: string;
  onChange: (value: string) => void;
  templateId?: string;
  templateName?: string;
  baseUrl?: string;
  disabled?: boolean;
  options?: readonly ProviderTemplateIconOption[];
  autoLabel?: string;
  inheritedIcon?: string | null;
  triggerTestId?: string;
  selectTestId?: string;
  menuTestId?: string;
}

export function ProviderTemplateIconPicker({
  value,
  onChange,
  templateId,
  templateName,
  baseUrl,
  disabled = false,
  options,
  autoLabel,
  inheritedIcon,
  triggerTestId = "template-edit-icon-trigger",
  selectTestId = "template-edit-icon",
  menuTestId = "template-edit-icon-menu",
}: ProviderTemplateIconPickerProps) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    function handleClickOutside(e: MouseEvent) {
      if (containerRef.current && !containerRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    }
    function handleKeyDown(e: KeyboardEvent) {
      if (e.key === "Escape") setOpen(false);
    }
    document.addEventListener("mousedown", handleClickOutside);
    document.addEventListener("keydown", handleKeyDown);
    return () => {
      document.removeEventListener("mousedown", handleClickOutside);
      document.removeEventListener("keydown", handleKeyDown);
    };
  }, [open]);

  const effectiveOptions = options ?? PROVIDER_TEMPLATE_ICON_OPTIONS;
  const currentOption = effectiveOptions.find((opt) => opt.id === value);
  const defaultAutoLabel = autoLabel || t("aiGatewayTemplateIconAuto", "Auto (Default)");
  const label = currentOption
    ? t(currentOption.labelKey, currentOption.fallbackLabel)
    : defaultAutoLabel;
  const effectiveAvatarIcon = value || inheritedIcon;

  return (
    <div className="relative w-full" ref={containerRef}>
      <select
        data-testid={selectTestId}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        disabled={disabled}
        className="sr-only"
        tabIndex={-1}
        aria-hidden="true"
      >
        <option value="">{defaultAutoLabel}</option>
        {effectiveOptions.map((opt) => (
          <option key={opt.id} value={opt.id}>
            {t(opt.labelKey, opt.fallbackLabel)}
          </option>
        ))}
      </select>

      <button
        type="button"
        data-testid={triggerTestId}
        aria-haspopup="listbox"
        aria-expanded={open}
        disabled={disabled}
        onClick={() => setOpen((prev) => !prev)}
        className="flex h-[38px] w-full items-center justify-between gap-2 rounded-lg border border-border bg-background px-3 text-sm text-foreground transition-all hover:border-primary/60 focus:border-primary focus:ring-2 focus:ring-primary/50 disabled:opacity-50 disabled:cursor-not-allowed"
      >
        <div className="flex items-center gap-2.5 min-w-0">
          <ProviderTemplateAvatar
            icon={effectiveAvatarIcon}
            templateId={templateId}
            templateName={templateName}
            baseUrl={baseUrl}
            size={24}
          />
          <span className="truncate text-sm font-medium text-foreground">
            {label}
          </span>
        </div>
        <ChevronDown
          className={`h-4 w-4 shrink-0 text-muted-foreground transition-transform duration-200 ${
            open ? "rotate-180" : ""
          }`}
        />
      </button>

      {open && (
        <div
          role="listbox"
          data-testid={menuTestId}
          className="absolute left-0 top-full z-50 mt-1.5 max-h-60 w-full min-w-[220px] overflow-y-auto rounded-xl border border-border/80 bg-popover p-1.5 text-popover-foreground shadow-lg animate-in fade-in-0 zoom-in-95"
        >
          <button
            type="button"
            role="option"
            aria-selected={value === ""}
            data-testid="template-icon-option-auto"
            onClick={() => {
              onChange("");
              setOpen(false);
            }}
            className={`flex w-full items-center justify-between rounded-lg px-2.5 py-2 text-left text-xs transition hover:bg-muted ${
              value === "" ? "bg-muted/70 font-semibold text-primary" : "text-foreground"
            }`}
          >
            <div className="flex items-center gap-2.5 min-w-0">
              <ProviderTemplateAvatar
                icon={inheritedIcon}
                templateId={templateId}
                templateName={templateName}
                size={28}
              />
              <span className="truncate">{defaultAutoLabel}</span>
            </div>
            {value === "" && <Check className="h-4 w-4 text-primary shrink-0" />}
          </button>

          <div className="my-1 border-t border-border/60" />

          {effectiveOptions.map((opt) => {
            const isSelected = value === opt.id;
            return (
              <button
                key={opt.id}
                type="button"
                role="option"
                aria-selected={isSelected}
                data-testid={`template-icon-option-${opt.id}`}
                onClick={() => {
                  onChange(opt.id);
                  setOpen(false);
                }}
                className={`flex w-full items-center justify-between rounded-lg px-2.5 py-2 text-left text-xs transition hover:bg-muted ${
                  isSelected ? "bg-muted/70 font-semibold text-primary" : "text-foreground"
                }`}
              >
                <div className="flex items-center gap-2.5 min-w-0">
                  <ProviderTemplateAvatar icon={opt.id} size={28} />
                  <span className="truncate">{t(opt.labelKey, opt.fallbackLabel)}</span>
                </div>
                {isSelected && <Check className="h-4 w-4 text-primary shrink-0" />}
              </button>
            );
          })}
        </div>
      )}
    </div>
  );
}
