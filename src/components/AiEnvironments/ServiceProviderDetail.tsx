import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  AlertTriangle,
  ArrowLeft,
  Check,
  ChevronDown,
  History,
  KeyRound,
  Loader2,
  Pencil,
  RotateCcw,
  Save,
  Settings2,
  Trash2,
  WandSparkles,
  X,
  Zap,
} from 'lucide-react';
import Editor from 'react-simple-code-editor';
import { highlight, languages } from 'prismjs';
import 'prismjs/components/prism-json';
import 'prismjs/themes/prism-tomorrow.css';
import { ServiceProviderAvatar } from './ServiceProviderAvatar';
import { ConfigJsonEditor } from './ConfigJsonEditor';
import { ModelMappingTable } from './ModelMappingTable';
import { Dialog, DialogClose, DialogContent, DialogHeader, DialogTitle, DialogTrigger } from '@/components/ui/dialog';
import { cn } from '@/lib/utils';
import { BuiltinProviderIcon, isBuiltinProviderIcon, resolveBuiltinProviderIcon, type BuiltinProviderIconKey } from './icons';

interface ClaudeModelMapping {
  family: string;
  display_name: string;
  upstream_model: string;
  supports_1m?: boolean;
  reasoning_effort?: string;
  supported_capabilities?: string[];
}

interface HistoryEntry {
  timestamp: number;
  content: string;
}

type JsonMode = 'claude' | 'generic' | 'opencode';

interface ServiceProviderDetailProps {
  provider: any;
  onChange: (changes: Partial<any>) => void;
  onSave: () => void;
  onActivate: () => void;
  onDelete: () => void;
  onBack: () => void;
  isActive?: boolean;
  t?: (key: string, fallback: string, options?: Record<string, any>) => string;
  onFetchModels?: (provider: any) => Promise<string[]>;
  jsonMode?: JsonMode;
  jsonValue?: string;
  jsonHistory?: HistoryEntry[];
  jsonError?: string | null;
  isRollbackMode?: boolean;
  onJsonChange?: (value: string) => void;
  onJsonError?: (error: string | null) => void;
  onRollback?: (entry: HistoryEntry) => void;
  onFormatJson?: () => void;
  onCancelRollback?: () => void;
  importedInactiveNotice?: string | null;
}

const ICON_OPTIONS = [
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
const AUTH_ENV_OPTIONS = ['ANTHROPIC_AUTH_TOKEN', 'ANTHROPIC_API_KEY'];
const CLAUDE_TOGGLE_FIELDS = [
  { key: 'claude_enable_tool_search', labelKey: 'enableToolSearch', fallback: 'Enable Tool Search' },
  { key: 'claude_auto_memory_enabled', labelKey: 'enableAutoMemory', fallback: 'Enable Auto Memory' },
  { key: 'claude_always_thinking_enabled', labelKey: 'alwaysEnableExtendedThinking', fallback: 'Always Enable Extended Thinking' },
  { key: 'claude_away_summary_enabled', labelKey: 'showAwaySummary', fallback: 'Show session summary after returning to terminal' },
  { key: 'claude_include_git_instructions', labelKey: 'includeGitInstructions', fallback: 'Include Git commit / PR instructions' },
] as const;

const modelEnvKeyByFamily: Record<string, string> = {
  haiku: 'ANTHROPIC_DEFAULT_HAIKU_MODEL',
  sonnet: 'ANTHROPIC_DEFAULT_SONNET_MODEL',
  opus: 'ANTHROPIC_DEFAULT_OPUS_MODEL',
};

const modelNameEnvKeyByFamily: Record<string, string> = {
  haiku: 'ANTHROPIC_DEFAULT_HAIKU_MODEL_NAME',
  sonnet: 'ANTHROPIC_DEFAULT_SONNET_MODEL_NAME',
  opus: 'ANTHROPIC_DEFAULT_OPUS_MODEL_NAME',
};

const modelCapabilitiesEnvKeyByFamily: Record<string, string> = {
  haiku: 'ANTHROPIC_DEFAULT_HAIKU_MODEL_SUPPORTED_CAPABILITIES',
  sonnet: 'ANTHROPIC_DEFAULT_SONNET_MODEL_SUPPORTED_CAPABILITIES',
  opus: 'ANTHROPIC_DEFAULT_OPUS_MODEL_SUPPORTED_CAPABILITIES',
};

function resolveClaudeEffectiveEffort(provider: any) {
  const providerEffort =
    typeof provider?.claude_reasoning_effort === 'string' && provider.claude_reasoning_effort.length > 0
      ? provider.claude_reasoning_effort
      : undefined;
  const selectedModel =
    typeof provider?.claude_default_model === 'string' && provider.claude_default_model.length > 0
      ? provider.claude_default_model
      : undefined;

  if (!selectedModel) {
    return providerEffort;
  }

  const normalizedSelected = selectedModel.endsWith('[1m]')
    ? selectedModel.slice(0, -4)
    : selectedModel;

  const rowOverride = (provider?.claude_model_mappings || []).find((mapping: ClaudeModelMapping) => {
    if (!mapping?.upstream_model) return false;
    if (mapping.upstream_model === selectedModel || mapping.upstream_model === normalizedSelected) {
      return true;
    }
    return !!mapping.supports_1m && `${mapping.upstream_model}[1m]` === selectedModel;
  })?.reasoning_effort;

  return rowOverride || providerEffort;
}

function buildClaudeSettingsJson(provider: any) {
  const env: Record<string, string> = {};
  const apiFormat = provider?.claude_api_format || 'anthropic_messages';
  const authKey = provider?.claude_auth_env_key || 'ANTHROPIC_AUTH_TOKEN';
  const apiKey = provider?.api_key || '';

  if (apiFormat === 'anthropic_messages') {
    env[authKey] = apiKey;
    if (provider?.base_url) {
      env.ANTHROPIC_BASE_URL = provider.base_url;
    }
  } else {
    env.ANTHROPIC_API_KEY = '{protocol-router-token}';
    env.ANTHROPIC_BASE_URL = `http://127.0.0.1:17687/anthropic/service-provider-${provider?.id || '{id}'}/v1`;
  }

  for (const mapping of provider?.claude_model_mappings || []) {
    const modelKey = modelEnvKeyByFamily[mapping.family];
    const nameKey = modelNameEnvKeyByFamily[mapping.family];
    const capabilitiesKey = modelCapabilitiesEnvKeyByFamily[mapping.family];
    if (!modelKey || !mapping.upstream_model) continue;
    env[modelKey] = mapping.supports_1m && mapping.family !== 'haiku'
      ? `${mapping.upstream_model}${mapping.upstream_model.includes('[1m]') ? '' : '[1m]'}`
      : mapping.upstream_model;
    if (mapping.display_name) {
      env[nameKey] = mapping.display_name;
    }
    if (capabilitiesKey && Array.isArray(mapping.supported_capabilities) && mapping.supported_capabilities.length > 0) {
      env[capabilitiesKey] = mapping.supported_capabilities.join(',');
    }
  }

  if (provider?.claude_default_model) {
    env.ANTHROPIC_MODEL = provider.claude_default_model;
  }

  const effort = resolveClaudeEffectiveEffort(provider);
  if (effort) {
    env.CLAUDE_CODE_EFFORT_LEVEL = effort;
  }

  if (provider?.claude_enable_tool_search) {
    env.ENABLE_TOOL_SEARCH = 'true';
  }

  const settings: Record<string, any> = {
    env,
    attribution: provider?.claude_enable_attribution ? undefined : { commit: '', pr: '' },
  };

  if (provider?.claude_auto_memory_enabled !== undefined) {
    settings.autoMemoryEnabled = !!provider.claude_auto_memory_enabled;
  }
  if (provider?.claude_always_thinking_enabled !== undefined) {
    settings.alwaysThinkingEnabled = !!provider.claude_always_thinking_enabled;
  }
  if (provider?.claude_away_summary_enabled !== undefined) {
    settings.awaySummaryEnabled = !!provider.claude_away_summary_enabled;
  }
  if (provider?.claude_include_git_instructions !== undefined) {
    settings.includeGitInstructions = !!provider.claude_include_git_instructions;
  }

  return JSON.stringify(settings, null, 2);
}

function IconPicker({
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
          {ICON_OPTIONS.map((icon) => {
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

function OpenCodeJsonPanel({
  value,
  history,
  jsonError,
  isRollbackMode,
  onChange,
  onRollback,
  onFormat,
  onCancelRollback,
  t,
}: {
  value: string;
  history: HistoryEntry[];
  jsonError?: string | null;
  isRollbackMode?: boolean;
  onChange?: (value: string) => void;
  onRollback?: (entry: HistoryEntry) => void;
  onFormat?: () => void;
  onCancelRollback?: () => void;
  t?: (key: string, fallback: string, options?: Record<string, any>) => string;
}) {
  const historyRef = useRef<HTMLDivElement>(null);
  const [showHistory, setShowHistory] = useState(false);

  useEffect(() => {
    function handleClickOutside(event: MouseEvent) {
      if (historyRef.current && !historyRef.current.contains(event.target as Node)) {
        setShowHistory(false);
      }
    }
    document.addEventListener('mousedown', handleClickOutside);
    return () => document.removeEventListener('mousedown', handleClickOutside);
  }, []);

  return (
    <>
      <div className="mb-3 flex items-center justify-between gap-3">
        <div className="text-sm text-muted-foreground">
          {t ? t('jsonEditHint', 'Edit the provider JSON directly for advanced OpenCode settings.') : 'Edit the provider JSON directly for advanced OpenCode settings.'}
        </div>
        <div className="flex items-center gap-2">
          <div className="relative" ref={historyRef}>
            <button type="button" onClick={() => setShowHistory((prev) => !prev)} className="acc-btn">
              <History className="w-3 h-3" />
              {t ? t('aiHistory', 'History') : 'History'}
            </button>
            {showHistory && (
              <div className="absolute right-0 top-full z-50 mt-2 w-80 max-h-96 overflow-hidden rounded-lg border bg-popover shadow-xl">
                <div className="flex items-center justify-between border-b bg-muted/30 p-3">
                  <span className="text-xs font-bold uppercase tracking-wider">
                    {t ? t('aiHistory', 'History') : 'History'}
                  </span>
                  <button type="button" onClick={() => setShowHistory(false)}>
                    <X className="w-4 h-4" />
                  </button>
                </div>
                <div className="max-h-[300px] overflow-y-auto p-1">
                  {history.length === 0 ? (
                    <div className="p-8 text-center text-xs text-muted-foreground">
                      {t ? t('noHistory', 'No history') : 'No history'}
                    </div>
                  ) : (
                    history.map((entry, index) => (
                      <div
                        key={`${entry.timestamp}-${index}`}
                        className="group mb-1 rounded-md border border-transparent p-2 transition-all hover:border-border hover:bg-muted/50"
                      >
                        <div className="mb-1 flex items-center justify-between">
                          <span className="text-[10px] font-mono text-muted-foreground">
                            {new Date(entry.timestamp).toLocaleString()}
                          </span>
                          <button
                            type="button"
                            onClick={() => {
                              onRollback?.(entry);
                              setShowHistory(false);
                            }}
                            className="flex items-center gap-1 text-[10px] text-primary hover:underline"
                          >
                            <RotateCcw className="w-2.5 h-2.5" />
                            {t ? t('rollback', 'Rollback') : 'Rollback'}
                          </button>
                        </div>
                        <div className="truncate rounded border border-border/50 bg-background/50 p-1.5 font-mono text-[10px] text-muted-foreground">
                          {entry.content.substring(0, 100)}...
                        </div>
                      </div>
                    ))
                  )}
                </div>
              </div>
            )}
          </div>
          <button type="button" onClick={onFormat} className="acc-btn">
            <WandSparkles className="w-3 h-3" />
            {t ? t('format', 'Format') : 'Format'}
          </button>
        </div>
      </div>

      {isRollbackMode && (
        <div className="mb-4 flex items-start gap-3 rounded-md border border-amber-200 bg-amber-50 p-3">
          <RotateCcw className="mt-0.5 h-4 w-4 text-amber-600" />
          <div className="space-y-1">
            <p className="text-sm font-semibold text-amber-800">
              {t ? t('rollbackModeTitle', 'History version loaded.') : 'History version loaded.'}
            </p>
            <p className="text-xs text-amber-700">
              {t ? t('rollbackModeDesc', 'Review this version before saving to apply the rollback.') : 'Review this version before saving to apply the rollback.'}
            </p>
          </div>
          <button
            type="button"
            onClick={onCancelRollback}
            className="ml-auto text-xs font-medium text-amber-800 hover:underline"
          >
            {t ? t('cancel', 'Cancel') : 'Cancel'}
          </button>
        </div>
      )}

      <div className={cn(
        'overflow-hidden rounded-md border bg-white font-mono text-sm shadow-inner transition-colors',
        isRollbackMode ? 'border-amber-500 ring-2 ring-amber-500' : jsonError ? 'border-destructive' : 'border-border',
      )}>
        <Editor
          value={value}
          onValueChange={(code) => onChange?.(code)}
          highlight={(code) => highlight(code, languages.json, 'json')}
          padding={16}
          style={{
            fontFamily: '"Fira Code", "Fira Mono", monospace',
            minHeight: '240px',
            backgroundColor: 'white',
            color: '#1a1a1a',
          }}
          className="focus:outline-none"
        />
      </div>
      {jsonError ? <p className="mt-2 text-xs text-destructive">{jsonError}</p> : null}
    </>
  );
}

export function ServiceProviderDetail({
  provider,
  onChange,
  onSave,
  onActivate,
  onDelete,
  onBack,
  isActive,
  t,
  onFetchModels,
  jsonMode,
  jsonValue,
  jsonHistory,
  jsonError,
  isRollbackMode,
  onJsonChange,
  onJsonError,
  onRollback,
  onFormatJson,
  onCancelRollback,
  importedInactiveNotice,
}: ServiceProviderDetailProps) {
  const [claudeJsonError, setClaudeJsonError] = useState<string | null>(null);
  const [claudeJsonDraft, setClaudeJsonDraft] = useState('');
  const [fetchingModels, setFetchingModels] = useState(false);
  const [fetchModelsError, setFetchModelsError] = useState<string | null>(null);

  const tool = provider?.tool;
  const isClaude = tool === 'claude';
  const isCodex = tool === 'codex';
  const isGemini = tool === 'gemini';
  const isOpenCode = tool === 'opencode';
  const connectionMode = provider?.claude_connection_mode || (
    provider?.claude_api_format === 'open_ai_chat' || provider?.claude_api_format === 'open_ai_responses'
      ? 'protocol_router'
      : 'native_anthropic'
  );
  const apiFormat = provider?.claude_api_format || (connectionMode === 'protocol_router' ? 'open_ai_chat' : 'anthropic_messages');

  const effectiveJsonMode = jsonMode || (isClaude ? 'claude' : isOpenCode ? 'opencode' : 'generic');

  const settingsJson = useMemo(() => {
    if (effectiveJsonMode === 'claude') return buildClaudeSettingsJson(provider);
    if (effectiveJsonMode === 'opencode') return jsonValue || '{}';
    return jsonValue || JSON.stringify(provider || {}, null, 2);
  }, [effectiveJsonMode, jsonValue, provider]);

  useEffect(() => {
    if (effectiveJsonMode !== 'claude') return;
    setClaudeJsonDraft(settingsJson);
    setClaudeJsonError(null);
  }, [effectiveJsonMode, settingsJson]);

  const handleClaudeJsonChange = useCallback((raw: string) => {
    setClaudeJsonDraft(raw);
    try {
      JSON.parse(raw);
      setClaudeJsonError(null);
    } catch (e: any) {
      setClaudeJsonError(e?.message || (t ? t('invalidJson', 'Invalid JSON') : 'Invalid JSON'));
    }
  }, [t]);

  const handleFetchModels = async () => {
    if (!onFetchModels) return;
    setFetchingModels(true);
    setFetchModelsError(null);
    try {
      const models = await onFetchModels(provider);
      onChange({ fetched_models: models });
      setFetchModelsError(null);
    } catch (e: any) {
      setFetchModelsError(
        e?.message || (t ? t('fetchModelsFailed', 'Failed to fetch models') : 'Failed to fetch models'),
      );
    } finally {
      setFetchingModels(false);
    }
  };

  const handleMappingChange = (mappings: ClaudeModelMapping[]) => {
    onChange({ claude_model_mappings: mappings });
  };

  const saveDisabled = effectiveJsonMode === 'claude' ? !!claudeJsonError : !!jsonError;
  const providerIdentifierLabel = isOpenCode
    ? (t ? t('providerIdentifier', 'Service Provider Identifier') : 'Service Provider Identifier')
    : (t ? t('providerIdentifier', 'Service Provider Identifier') : 'Service Provider Identifier');

  return (
    <div className="flex h-full flex-col bg-background">
      <div className="shrink-0 border-b bg-card px-5 py-4">
        <div className="flex items-center gap-3">
          <button type="button" className="acc-btn" onClick={onBack} title={t ? t('back', 'Back') : 'Back'}>
            <ArrowLeft className="h-3.5 w-3.5" />
            {t ? t('back', 'Back') : 'Back'}
          </button>
          <IconPicker
            value={provider?.icon}
            name={provider?.name}
            providerId={provider?.id}
            tool={provider?.tool}
            onChange={(icon) => onChange({ icon })}
            t={t}
            triggerClassName="h-auto w-auto border-0 bg-transparent p-0 hover:border-0 hover:bg-transparent"
            trigger={(
              <div className="relative">
                <ServiceProviderAvatar
                  icon={provider?.icon}
                  name={provider?.name || ''}
                  id={provider?.id || ''}
                  tool={provider?.tool}
                  size={40}
                />
                <div className="pointer-events-none absolute -bottom-1 -right-1 rounded-full border border-border bg-background p-1 text-muted-foreground">
                  <Pencil className="h-3 w-3" />
                </div>
              </div>
            )}
          />
          <div className="min-w-0 flex-1">
            <div className="flex items-center gap-2">
              <h3 className="truncate text-base font-semibold">
                {provider?.name || t?.('newPreset', 'New Service Provider')}
              </h3>
              {isActive ? (
                <span className="badge-pill bg-green-500/10 text-green-700">
                  {t ? t('active', 'Active') : 'Active'}
                </span>
              ) : null}
            </div>
            <p className="text-xs capitalize text-muted-foreground">{provider?.tool}</p>
          </div>
          <div className="flex items-center gap-2">
            {!isActive ? (
              <button type="button" className="acc-panel-btn" onClick={onActivate}>
                <Zap className="h-4 w-4" />
                {t ? t('activateServiceProvider', 'Activate') : 'Activate'}
              </button>
            ) : null}
            <button type="button" className="acc-panel-btn danger" onClick={onDelete}>
              <Trash2 className="h-4 w-4" />
              {t ? t('delete', 'Delete') : 'Delete'}
            </button>
            <button type="button" className="acc-panel-btn primary" onClick={onSave} disabled={saveDisabled}>
              <Save className="h-4 w-4" />
              {t ? t('save', 'Save') : 'Save'}
            </button>
          </div>
        </div>
      </div>

      <div className="flex-1 overflow-y-auto p-5">
        <div className="space-y-6">
          {importedInactiveNotice ? (
            <div className="rounded-lg border-2 border-amber-500/70 bg-amber-100/80 px-4 py-3 shadow-sm">
              <div className="flex items-start gap-2.5">
                <AlertTriangle className="mt-0.5 h-5 w-5 shrink-0 text-amber-800" />
                <div>
                  <p className="text-sm font-extrabold uppercase tracking-wide text-amber-900">
                    {t ? t('importedButInactiveTitle', 'Imported but inactive') : 'Imported but inactive'}
                  </p>
                  <p className="mt-1 text-sm font-medium text-amber-900/90">{importedInactiveNotice}</p>
                </div>
              </div>
            </div>
          ) : null}

          <section>
            <div className="acc-section-head">
              <Settings2 />
              <h5>{t ? t('basicInfo', 'Basic Info') : 'Basic Info'}</h5>
            </div>
            <div className="field-grid col-2">
              <div className="field">
                <label className="required">{t ? t('providerName', 'Service Provider Name') : 'Service Provider Name'}</label>
                <input value={provider?.name || ''} onChange={(e) => onChange({ name: e.target.value })} />
              </div>
              <div className="field">
                <label>{providerIdentifierLabel}</label>
                {isOpenCode ? (
                  <input
                    value={provider?.provider_key || ''}
                    onChange={(e) => onChange({ provider_key: e.target.value.replace(/[^a-zA-Z]/g, '') })}
                  />
                ) : (
                  <input
                    value={provider?.code || ''}
                    onChange={(e) => onChange({ code: e.target.value.toLowerCase().replace(/[^a-z0-9_-]/g, '') })}
                  />
                )}
              </div>
              <div className="field full-span">
                <label>{t ? t('providerRemark', 'Service Provider Remark') : 'Service Provider Remark'}</label>
                <textarea
                  rows={3}
                  value={provider?.remark || ''}
                  onChange={(e) => onChange({ remark: e.target.value })}
                />
              </div>
              <div className="field full-span">
                <label>{t ? t('apiKey', 'API Key') : 'API Key'}</label>
                <input type="text" value={provider?.api_key || ''} onChange={(e) => onChange({ api_key: e.target.value })} />
              </div>
              <div className="field full-span">
                <label>{t ? t('baseUrl', 'Base URL') : 'Base URL'}</label>
                <input value={provider?.base_url || ''} onChange={(e) => onChange({ base_url: e.target.value })} />
              </div>
              {!isClaude ? (
                <div className="field full-span">
                  <label>{t ? t('model', 'Primary Model') : 'Primary Model'}</label>
                  <input value={provider?.model || ''} onChange={(e) => onChange({ model: e.target.value })} />
                </div>
              ) : null}
            </div>
          </section>

          <section>
            <div className="acc-section-head">
              <KeyRound />
              <h5>{t ? t('toolSpecificConfig', 'Tool Specific Config') : 'Tool Specific Config'}</h5>
            </div>

            {isClaude ? (
              <div className="space-y-5">
                <div className="field-grid col-2">
                  <div className="field">
                    <label>{t ? t('connectionMode', 'Connection Mode') : 'Connection Mode'}</label>
                    <select
                      value={connectionMode}
                      onChange={(e) => onChange({
                        claude_connection_mode: e.target.value,
                        claude_api_format: e.target.value === 'protocol_router' ? 'open_ai_chat' : 'anthropic_messages',
                      })}
                    >
                      <option value="native_anthropic">{t ? t('nativeAnthropicMode', 'Native Anthropic') : 'Native Anthropic'}</option>
                      <option value="protocol_router">{t ? t('protocolRouterMode', 'Protocol Router') : 'Protocol Router'}</option>
                    </select>
                  </div>
                  <div className="field">
                    <label>{t ? t('apiFormat', 'API Format') : 'API Format'}</label>
                    <select
                      value={apiFormat}
                      onChange={(e) => onChange({
                        claude_api_format: e.target.value,
                        claude_connection_mode: e.target.value === 'anthropic_messages' ? 'native_anthropic' : 'protocol_router',
                      })}
                    >
                      {connectionMode === 'native_anthropic' ? (
                        <option value="anthropic_messages">{t ? t('anthropicMessagesFormat', 'Anthropic Messages') : 'Anthropic Messages'}</option>
                      ) : null}
                      {connectionMode === 'protocol_router' ? (
                        <>
                          <option value="open_ai_chat">{t ? t('openAiChatFormat', 'OpenAI Chat') : 'OpenAI Chat'}</option>
                          <option value="open_ai_responses">{t ? t('openAiResponsesFormat', 'OpenAI Responses') : 'OpenAI Responses'}</option>
                        </>
                      ) : null}
                    </select>
                  </div>
                  {connectionMode === 'native_anthropic' ? (
                    <div className="field">
                      <label>{t ? t('authEnvKey', 'Auth Env Key') : 'Auth Env Key'}</label>
                      <select
                        value={provider?.claude_auth_env_key || 'ANTHROPIC_AUTH_TOKEN'}
                        onChange={(e) => onChange({ claude_auth_env_key: e.target.value })}
                      >
                        {AUTH_ENV_OPTIONS.map((key) => <option key={key} value={key}>{key}</option>)}
                      </select>
                    </div>
                  ) : null}
                  <div className="field">
                    <label>{t ? t('defaultModel', 'Default Model') : 'Default Model'}</label>
                    <input
                      value={provider?.claude_default_model || ''}
                      onChange={(e) => onChange({ claude_default_model: e.target.value || undefined })}
                    />
                  </div>
                  <div className="field">
                    <label>{t ? t('reasoningEffort', 'Reasoning Effort') : 'Reasoning Effort'}</label>
                    <input
                      value={provider?.claude_reasoning_effort || ''}
                      onChange={(e) => onChange({ claude_reasoning_effort: e.target.value || undefined })}
                      placeholder={t ? t('claudeReasoningEffortPlaceholder', 'high / xhigh / max / auto / custom') : 'high / xhigh / max / auto / custom'}
                    />
                  </div>
                </div>

                <div>
                  <div className="mb-3 flex items-center justify-between gap-3">
                    <div className="text-sm font-semibold">{t ? t('modelMappings', 'Model Mappings') : 'Model Mappings'}</div>
                    <button type="button" className="acc-btn" onClick={handleFetchModels} disabled={fetchingModels}>
                      {fetchingModels ? <Loader2 className="h-3 w-3 animate-spin" /> : null}
                      {fetchingModels
                        ? (t ? t('fetchingModels', 'Fetching...') : 'Fetching...')
                        : (t ? t('fetchModels', 'Fetch Models') : 'Fetch Models')}
                    </button>
                  </div>
                  <ModelMappingTable
                    mappings={provider?.claude_model_mappings || []}
                    onChange={handleMappingChange}
                    fetchedModels={provider?.fetched_models}
                    t={t ? (key, fallback) => t(key, fallback) : undefined}
                  />
                  {fetchModelsError ? (
                    <p className="mt-2 text-xs text-destructive">
                      {t ? t('fetchModelsFailedHint', 'Model list fetch failed. Check API endpoint, API key, or upstream service status.') : 'Model list fetch failed. Check API endpoint, API key, or upstream service status.'}
                      {' '}
                      {fetchModelsError}
                    </p>
                  ) : null}
                </div>

                <div className="grid gap-3 md:grid-cols-2">
                  {CLAUDE_TOGGLE_FIELDS.map((field) => (
                    <label key={field.key} className="checkbox-row info mb-0">
                      <input
                        type="checkbox"
                        checked={!!provider?.[field.key]}
                        onChange={(e) => onChange({ [field.key]: e.target.checked })}
                      />
                      <span>
                        <span className="label">{t ? t(field.labelKey, field.fallback) : field.fallback}</span>
                      </span>
                    </label>
                  ))}
                  <label className="checkbox-row info mb-0">
                    <input
                      type="checkbox"
                      checked={!provider?.claude_enable_attribution}
                      onChange={(e) => onChange({ claude_enable_attribution: !e.target.checked })}
                    />
                    <span>
                      <span className="label">{t ? t('hideAttribution', 'Hide AI Attribution') : 'Hide AI Attribution'}</span>
                    </span>
                  </label>
                </div>
              </div>
            ) : null}

            {isCodex ? (
              <div className="field-grid col-2">
                <label className="checkbox-row info mb-0 full-span">
                  <input
                    type="checkbox"
                    checked={!!provider?.disable_response_storage}
                    onChange={(e) => onChange({ disable_response_storage: e.target.checked })}
                  />
                  <span>
                    <span className="label">{t ? t('disableResponseStorage', 'Disable Response Storage') : 'Disable Response Storage'}</span>
                    <span className="desc">{t ? t('disableResponseStorageDesc', 'Do not store responses locally for privacy.') : 'Do not store responses locally for privacy.'}</span>
                  </span>
                </label>
                <div className="field">
                  <label>{t ? t('personality', 'Personality') : 'Personality'}</label>
                  <select value={provider?.personality || ''} onChange={(e) => onChange({ personality: e.target.value || undefined })}>
                    <option value="">{t ? t('personalityDefault', 'Default') : 'Default'}</option>
                    <option value="pragmatic">{t ? t('personalityPragmatic', 'Pragmatic') : 'Pragmatic'}</option>
                    <option value="chatty">{t ? t('personalityChatty', 'Chatty') : 'Chatty'}</option>
                  </select>
                </div>
                <div className="field">
                  <label>{t ? t('wireApi', 'Wire API Format') : 'Wire API Format'}</label>
                  <select value={provider?.wire_api || ''} onChange={(e) => onChange({ wire_api: e.target.value || undefined })}>
                    <option value="">{t ? t('wireApiDefault', 'Default') : 'Default'}</option>
                    <option value="chat">{t ? t('wireApiChat', 'Chat (Legacy)') : 'Chat (Legacy)'}</option>
                    <option value="responses">{t ? t('wireApiResponses', 'Responses (New)') : 'Responses (New)'}</option>
                  </select>
                </div>
                <div className="field">
                  <label>{t ? t('reasoningEffort', 'Reasoning Effort') : 'Reasoning Effort'}</label>
                  <select value={provider?.model_reasoning_effort || ''} onChange={(e) => onChange({ model_reasoning_effort: e.target.value || undefined })}>
                    <option value="">{t ? t('reasoningEffortDefault', 'Default') : 'Default'}</option>
                    <option value="minimal">{t ? t('reasoningEffortMinimal', 'Minimal') : 'Minimal'}</option>
                    <option value="low">{t ? t('reasoningEffortLow', 'Low') : 'Low'}</option>
                    <option value="medium">{t ? t('reasoningEffortMedium', 'Medium') : 'Medium'}</option>
                    <option value="high">{t ? t('reasoningEffortHigh', 'High') : 'High'}</option>
                    <option value="xhigh">{t ? t('reasoningEffortXHigh', 'XHigh') : 'XHigh'}</option>
                  </select>
                </div>
                <div className="field">
                  <label>{t ? t('reasoningSummary', 'Reasoning Summary') : 'Reasoning Summary'}</label>
                  <select value={provider?.model_reasoning_summary || ''} onChange={(e) => onChange({ model_reasoning_summary: e.target.value || undefined })}>
                    <option value="">{t ? t('reasoningSummaryAuto', 'Auto') : 'Auto'}</option>
                    <option value="concise">{t ? t('reasoningSummaryConcise', 'Concise') : 'Concise'}</option>
                    <option value="detailed">{t ? t('reasoningSummaryDetailed', 'Detailed') : 'Detailed'}</option>
                    <option value="none">{t ? t('reasoningSummaryNone', 'None') : 'None'}</option>
                  </select>
                </div>
                <div className="field">
                  <label>{t ? t('approvalPolicy', 'Approval Policy') : 'Approval Policy'}</label>
                  <select value={provider?.approval_policy || ''} onChange={(e) => onChange({ approval_policy: e.target.value || undefined })}>
                    <option value="">{t ? t('approvalPolicyDefault', 'Default') : 'Default'}</option>
                    <option value="untrusted">{t ? t('approvalPolicyUntrusted', 'Untrusted') : 'Untrusted'}</option>
                    <option value="on-failure">{t ? t('approvalPolicyOnFailure', 'On Failure') : 'On Failure'}</option>
                    <option value="on-request">{t ? t('approvalPolicyOnRequest', 'On Request') : 'On Request'}</option>
                    <option value="never">{t ? t('approvalPolicyNever', 'Never') : 'Never'}</option>
                  </select>
                </div>
                <div className="field">
                  <label>{t ? t('sandboxMode', 'Sandbox Mode') : 'Sandbox Mode'}</label>
                  <select value={provider?.sandbox_mode || ''} onChange={(e) => onChange({ sandbox_mode: e.target.value || undefined })}>
                    <option value="">{t ? t('sandboxModeDefault', 'Default') : 'Default'}</option>
                    <option value="read-only">{t ? t('sandboxModeReadOnly', 'Read Only') : 'Read Only'}</option>
                    <option value="workspace-write">{t ? t('sandboxModeWorkspaceWrite', 'Workspace Write') : 'Workspace Write'}</option>
                  </select>
                </div>
              </div>
            ) : null}

            {isGemini ? (
              <div className="field-grid col-2">
                <div className="field">
                  <label>{t ? t('geminiAuthType', 'Gemini Auth Type') : 'Gemini Auth Type'}</label>
                  <select value={provider?.gemini_auth_type || ''} onChange={(e) => onChange({ gemini_auth_type: e.target.value || undefined })}>
                    <option value="">{t ? t('geminiAuthDefault', 'Default') : 'Default'}</option>
                    <option value="gemini-api-key">{t ? t('geminiAuthApiKey', 'API Key') : 'API Key'}</option>
                    <option value="oauth-personal">{t ? t('geminiAuthOAuth', 'OAuth Personal') : 'OAuth Personal'}</option>
                  </select>
                </div>
                <div className="field">
                  <label>{t ? t('theme', 'Theme') : 'Theme'}</label>
                  <select value={provider?.theme || ''} onChange={(e) => onChange({ theme: e.target.value || undefined })}>
                    <option value="">{t ? t('themeDefault', 'Default') : 'Default'}</option>
                    <option value="Default">{t ? t('themeDefault', 'Default') : 'Default'}</option>
                    <option value="GitHub Dark">{t ? t('themeGitHubDark', 'GitHub Dark') : 'GitHub Dark'}</option>
                    <option value="Light">{t ? t('themeLight', 'Light') : 'Light'}</option>
                  </select>
                </div>
                <div className="field">
                  <label>{t ? t('defaultApprovalMode', 'Default Approval Mode') : 'Default Approval Mode'}</label>
                  <select value={provider?.default_approval_mode || ''} onChange={(e) => onChange({ default_approval_mode: e.target.value || undefined })}>
                    <option value="">{t ? t('defaultApprovalModeDefault', 'Default') : 'Default'}</option>
                    <option value="auto_edit">{t ? t('defaultApprovalModeAutoEdit', 'Auto Edit') : 'Auto Edit'}</option>
                    <option value="plan">{t ? t('defaultApprovalModePlan', 'Plan') : 'Plan'}</option>
                  </select>
                </div>
                <label className="checkbox-row info mb-0">
                  <input
                    type="checkbox"
                    checked={!!provider?.vim_mode}
                    onChange={(e) => onChange({ vim_mode: e.target.checked })}
                  />
                  <span>
                    <span className="label">{t ? t('vimMode', 'Vim Mode') : 'Vim Mode'}</span>
                    <span className="desc">{t ? t('vimModeDesc', 'Enable Vim keybindings.') : 'Enable Vim keybindings.'}</span>
                  </span>
                </label>
              </div>
            ) : null}

            {isOpenCode ? (
              <div className="field-grid col-2">
                <div className="field">
                  <label>{t ? t('defaultModel', 'Default Model') : 'Default Model'}</label>
                  <input value={provider?.opencode_default_model || ''} onChange={(e) => onChange({ opencode_default_model: e.target.value })} />
                </div>
                <div className="field">
                  <label>{t ? t('defaultAgent', 'Default Agent') : 'Default Agent'}</label>
                  <input value={provider?.opencode_default_agent || ''} onChange={(e) => onChange({ opencode_default_agent: e.target.value })} />
                </div>
                <div className="field">
                  <label>{t ? t('sessionsDir', 'Sessions Directory') : 'Sessions Directory'}</label>
                  <input value={provider?.opencode_sessions_dir || ''} onChange={(e) => onChange({ opencode_sessions_dir: e.target.value })} />
                </div>
                <div className="field">
                  <label>{t ? t('smallModel', 'Small Model') : 'Small Model'}</label>
                  <input value={provider?.small_model || ''} onChange={(e) => onChange({ small_model: e.target.value })} />
                </div>
                <div className="field">
                  <label>{t ? t('requestTimeout', 'Request Timeout') : 'Request Timeout'}</label>
                  <input
                    type="number"
                    value={provider?.timeout ?? ''}
                    onChange={(e) => onChange({ timeout: e.target.value ? parseInt(e.target.value, 10) : undefined })}
                  />
                </div>
                <div className="field">
                  <label>{t ? t('shareMode', 'Share Mode') : 'Share Mode'}</label>
                  <select value={provider?.share_mode || ''} onChange={(e) => onChange({ share_mode: e.target.value || undefined })}>
                    <option value="">{t ? t('shareModeManual', 'Manual') : 'Manual'}</option>
                    <option value="manual">{t ? t('shareModeManual', 'Manual') : 'Manual'}</option>
                    <option value="auto">{t ? t('shareModeAuto', 'Auto') : 'Auto'}</option>
                    <option value="disabled">{t ? t('shareModeDisabled', 'Disabled') : 'Disabled'}</option>
                  </select>
                </div>
              </div>
            ) : null}
          </section>

          {(isCodex || isGemini) ? (
            <section>
              <div className="acc-section-head">
                <Settings2 />
                <h5>{t ? t('managedConfig', 'Managed Config') : 'Managed Config'}</h5>
              </div>
              <label className="checkbox-row info mb-0">
                <input
                  type="checkbox"
                  checked={provider?.env_managed !== false}
                  onChange={(e) => onChange({ env_managed: e.target.checked })}
                />
                <span>
                  <span className="label">{t ? t('envManagedToggle', 'Enable Managed Config') : 'Enable Managed Config'}</span>
                  <span className="desc">
                    {provider?.env_managed !== false
                      ? (t ? t('envManagedEnabledDesc', 'Applying this provider will also update the CLI config automatically.') : 'Applying this provider will also update the CLI config automatically.')
                      : (t ? t('envManagedDisabledDesc', 'Managed config is disabled. CLI config must be maintained manually.') : 'Managed config is disabled. CLI config must be maintained manually.')}
                  </span>
                </span>
              </label>
            </section>
          ) : null}

          <section>
            <div className="acc-section-head">
              <Settings2 />
              <h5>{t ? t('configurationJson', 'Configuration JSON') : 'Configuration JSON'}</h5>
            </div>

            {effectiveJsonMode === 'claude' ? (
              <>
                <ConfigJsonEditor
                  value={claudeJsonDraft}
                  onChange={handleClaudeJsonChange}
                  onError={setClaudeJsonError}
                  t={t ? (key, fallback) => t(key, fallback) : undefined}
                />
                {claudeJsonError ? <p className="mt-2 text-xs text-destructive">{claudeJsonError}</p> : null}
              </>
            ) : null}

            {effectiveJsonMode === 'generic' ? (
              <ConfigJsonEditor
                value={settingsJson}
                onChange={(value) => onJsonChange?.(value)}
                onError={(error) => onJsonError?.(error)}
                t={t ? (key, fallback) => t(key, fallback) : undefined}
              />
            ) : null}

            {effectiveJsonMode === 'opencode' ? (
              <OpenCodeJsonPanel
                value={settingsJson}
                history={jsonHistory || []}
                jsonError={jsonError}
                isRollbackMode={isRollbackMode}
                onChange={onJsonChange}
                onRollback={onRollback}
                onFormat={onFormatJson}
                onCancelRollback={onCancelRollback}
                t={t}
              />
            ) : null}
          </section>
        </div>
      </div>
    </div>
  );
}
