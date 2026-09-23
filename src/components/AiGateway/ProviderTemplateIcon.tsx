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

export function resolveProviderTemplateIcon(
  icon?: string | null,
  templateId?: string | null,
  templateName?: string | null,
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
  size?: number;
  className?: string;
  style?: React.CSSProperties;
  title?: string;
}

export function ProviderTemplateAvatar({
  icon,
  templateId,
  templateName,
  size = 36,
  className,
  style,
  title,
}: ProviderTemplateAvatarProps) {
  const resolved = resolveProviderTemplateIcon(icon, templateId, templateName);
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
  disabled?: boolean;
}

export function ProviderTemplateIconPicker({
  value,
  onChange,
  templateId,
  templateName,
  disabled = false,
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

  const currentOption = PROVIDER_TEMPLATE_ICON_OPTIONS.find((opt) => opt.id === value);
  const label = currentOption
    ? t(currentOption.labelKey, currentOption.fallbackLabel)
    : t("aiGatewayTemplateIconAuto", "Auto (Default)");

  return (
    <div className="relative w-full" ref={containerRef}>
      <select
        data-testid="template-edit-icon"
        value={value}
        onChange={(e) => onChange(e.target.value)}
        disabled={disabled}
        className="sr-only"
        tabIndex={-1}
        aria-hidden="true"
      >
        <option value="">{t("aiGatewayTemplateIconAuto", "Auto (Default)")}</option>
        {PROVIDER_TEMPLATE_ICON_OPTIONS.map((opt) => (
          <option key={opt.id} value={opt.id}>
            {t(opt.labelKey, opt.fallbackLabel)}
          </option>
        ))}
      </select>

      <button
        type="button"
        data-testid="template-edit-icon-trigger"
        aria-haspopup="listbox"
        aria-expanded={open}
        disabled={disabled}
        onClick={() => setOpen((prev) => !prev)}
        className="flex h-[38px] w-full items-center justify-between gap-2 rounded-lg border border-border bg-background px-3 text-sm text-foreground transition-all hover:border-primary/60 focus:border-primary focus:ring-2 focus:ring-primary/50 disabled:opacity-50 disabled:cursor-not-allowed"
      >
        <div className="flex items-center gap-2.5 min-w-0">
          <ProviderTemplateAvatar
            icon={value}
            templateId={templateId}
            templateName={templateName}
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
          data-testid="template-edit-icon-menu"
          className="absolute left-0 top-full z-50 mt-1.5 w-full min-w-[220px] rounded-xl border border-border/80 bg-popover p-1.5 text-popover-foreground shadow-lg animate-in fade-in-0 zoom-in-95"
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
                templateId={templateId}
                templateName={templateName}
                size={28}
              />
              <span className="truncate">{t("aiGatewayTemplateIconAuto", "Auto (Default)")}</span>
            </div>
            {value === "" && <Check className="h-4 w-4 text-primary shrink-0" />}
          </button>

          <div className="my-1 border-t border-border/60" />

          {PROVIDER_TEMPLATE_ICON_OPTIONS.map((opt) => {
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
