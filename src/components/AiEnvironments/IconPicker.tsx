import { Check, ChevronDown } from 'lucide-react';
import React from 'react';
import { Dialog, DialogClose, DialogContent, DialogHeader, DialogTitle, DialogTrigger } from '@/components/ui/dialog';
import { cn } from '@/lib/utils';
import {
  BuiltinProviderIcon,
  isBuiltinProviderIcon,
  resolveBuiltinProviderIcon,
  type BuiltinProviderIconKey,
} from './icons';

export const PROVIDER_ICON_OPTIONS = [
  { value: 'builtin:claude', labelKey: 'providerIconClaude', fallback: 'Claude' },
  { value: 'builtin:chatgpt', labelKey: 'providerIconChatgpt', fallback: 'ChatGPT' },
  { value: 'builtin:gemini', labelKey: 'providerIconGemini', fallback: 'Gemini' },
  { value: 'builtin:opencode', labelKey: 'providerIconOpenCode', fallback: 'OpenCode' },
  { value: 'builtin:bailian', labelKey: 'providerIconBailian', fallback: 'Bailian' },
  { value: 'builtin:tencent', labelKey: 'providerIconTencent', fallback: 'Tencent Hunyuan' },
  { value: 'builtin:baidu', labelKey: 'providerIconBaidu', fallback: 'Baidu Qianfan' },
  { value: 'builtin:volcengine', labelKey: 'providerIconVolcengine', fallback: 'Volcengine' },
  { value: 'builtin:doubao', labelKey: 'providerIconDoubao', fallback: 'Doubao' },
  { value: 'builtin:deepseek', labelKey: 'providerIconDeepSeek', fallback: 'DeepSeek' },
  { value: 'builtin:zhipu', labelKey: 'providerIconZhipu', fallback: 'Zhipu' },
  { value: 'builtin:kimi', labelKey: 'providerIconKimi', fallback: 'Kimi' },
  { value: 'builtin:minimax', labelKey: 'providerIconMiniMax', fallback: 'MiniMax' },
  { value: 'builtin:stepfun', labelKey: 'providerIconStepFun', fallback: 'StepFun' },
  { value: 'builtin:xfyun', labelKey: 'providerIconXFYun', fallback: 'XFYun Spark' },
  { value: 'builtin:sensenova', labelKey: 'providerIconSenseNova', fallback: 'SenseNova' },
  { value: 'builtin:lingyi', labelKey: 'providerIconLingyi', fallback: '01.AI' },
] as const;

export function IconPicker({
  value,
  name,
  providerId,
  tool,
  onChange,
  t,
  triggerClassName,
  trigger,
}: {
  value?: string;
  name?: string;
  providerId?: string;
  tool?: string;
  onChange: (value?: string) => void;
  t?: (key: string, fallback: string, options?: Record<string, any>) => string;
  triggerClassName?: string;
  trigger?: React.ReactNode;
}) {
  const autoBuiltinIcon = resolveBuiltinProviderIcon({ icon: value, name, id: providerId, tool });
  const selectedLabel = value || (t ? t('iconAuto', 'Auto') : 'Auto');
  const renderPreview = (iconValue?: string, label?: string) => {
    if (iconValue && isBuiltinProviderIcon(iconValue)) {
      return <BuiltinProviderIcon icon={iconValue as BuiltinProviderIconKey} className="h-5 w-5" />;
    }
    return <span className="text-sm font-semibold leading-none">{label || iconValue}</span>;
  };

  return (
    <Dialog>
      <DialogTrigger asChild>
        <button
          type="button"
          className={cn(
            'flex h-10 w-full items-center justify-between rounded-md border border-border bg-background px-3 text-left text-sm text-foreground transition-colors hover:border-foreground/30',
            triggerClassName,
          )}
        >
          {trigger || (
            <>
              <span className="truncate">{selectedLabel}</span>
              <ChevronDown className="h-4 w-4 shrink-0 text-muted-foreground" />
            </>
          )}
        </button>
      </DialogTrigger>
      <DialogContent className="max-w-2xl border-slate-200 bg-white p-0 text-slate-900">
        <DialogHeader className="border-b border-slate-200 px-5 py-4 text-left">
          <DialogTitle className="text-base text-slate-900">
            {t ? t('selectIcon', 'Select icon') : 'Select icon'}
          </DialogTitle>
        </DialogHeader>
        <div className="grid grid-cols-2 gap-2 px-5 py-5 sm:grid-cols-3 lg:grid-cols-4">
          <DialogClose asChild>
            <button
              type="button"
              onClick={() => onChange(undefined)}
              className={cn(
                'flex h-16 items-center gap-3 rounded-md border px-3 text-left text-sm transition-colors',
                !value
                  ? 'border-slate-900 bg-slate-50 text-slate-900'
                  : 'border-slate-200 bg-white text-slate-700 hover:border-slate-300 hover:bg-slate-50 hover:text-slate-900',
              )}
            >
              <span className="inline-flex h-9 w-9 items-center justify-center rounded-xl border border-slate-200 bg-gradient-to-b from-white to-slate-50 text-slate-700 shadow-sm">
                {autoBuiltinIcon ? (
                  <BuiltinProviderIcon icon={autoBuiltinIcon} className="h-5 w-5" />
                ) : (
                  <span className="text-sm font-semibold leading-none">A</span>
                )}
              </span>
              <span>{t ? t('iconAuto', 'Auto') : 'Auto'}</span>
            </button>
          </DialogClose>
          {PROVIDER_ICON_OPTIONS.map((icon) => {
            const selected = value === icon.value;
            return (
              <DialogClose key={icon.value} asChild>
                <button
                  type="button"
                  onClick={() => onChange(icon.value)}
                  className={cn(
                    'flex h-16 items-center gap-3 rounded-md border px-3 text-left text-sm transition-colors',
                    selected
                      ? 'border-slate-900 bg-slate-50 text-slate-900'
                      : 'border-slate-200 bg-white text-slate-700 hover:border-slate-300 hover:bg-slate-50 hover:text-slate-900',
                  )}
                >
                  <span className="inline-flex h-10 w-10 shrink-0 items-center justify-center overflow-hidden rounded-xl border border-slate-200 bg-gradient-to-b from-white to-slate-50 text-slate-700 shadow-sm">
                    {renderPreview(icon.value, icon.fallback)}
                  </span>
                  <span className="flex min-w-0 flex-1 items-center justify-between gap-2">
                    <span className="truncate">{t ? t(icon.labelKey, icon.fallback) : icon.fallback}</span>
                    {selected && <Check className="h-3.5 w-3.5 shrink-0 text-slate-900" />}
                  </span>
                </button>
              </DialogClose>
            );
          })}
        </div>
      </DialogContent>
    </Dialog>
  );
}
