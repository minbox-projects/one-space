export type ServiceProviderPresetEndpoints = {
  openai_base_url?: string;
  anthropic_base_url?: string;
  gemini_base_url?: string;
};

export type ServiceProviderPresetRecord = {
  id: string;
  name: string;
  description?: string;
  icon?: string;
  endpoints: ServiceProviderPresetEndpoints;
  template?: Record<string, any>;
  created_at: number;
  updated_at: number;
};

export type ServiceProviderPresetsState = {
  presets: ServiceProviderPresetRecord[];
};

export type PresetTool = 'claude' | 'codex' | 'gemini' | 'opencode';

export type ProviderPresetDraft = {
  id: string;
  name: string;
  tool: string;
  api_key: string;
  base_url?: string;
  model?: string;
  icon?: string;
  code?: string;
  provider_key?: string;
  is_enabled?: boolean;
  env_managed?: boolean;
  favorite_at?: number | null;
  history?: unknown[];
  fetched_models?: string[];
  claude_api_format?: string;
  claude_connection_mode?: string;
  tool_config?: Record<string, any>;
  options?: Record<string, any>;
  [key: string]: any;
};

const INSTANCE_KEYS = new Set([
  'id',
  'tool',
  'api_key',
  'code',
  'provider_key',
  'is_enabled',
  'env_managed',
  'favorite_at',
  'history',
  'fetched_models',
  'base_url',
  'baseURL',
  'options',
  'models',
]);

function stringOrEmpty(value: unknown) {
  return typeof value === 'string' ? value.trim() : '';
}

function isOpenAiClaudeDraft(draft: Partial<ProviderPresetDraft>) {
  const apiFormat = draft.claude_api_format || draft.tool_config?.claude_api_format;
  const connectionMode = draft.claude_connection_mode || draft.tool_config?.claude_connection_mode;
  return (
    connectionMode === 'protocol_router' ||
    apiFormat === 'open_ai_chat' ||
    apiFormat === 'open_ai_responses'
  );
}

function endpointForPreset(
  preset: ServiceProviderPresetRecord,
  activeTool: string,
  draft: Partial<ProviderPresetDraft>,
) {
  if (activeTool === 'claude') {
    return isOpenAiClaudeDraft(draft)
      ? stringOrEmpty(preset.endpoints.openai_base_url)
      : stringOrEmpty(preset.endpoints.anthropic_base_url);
  }
  if (activeTool === 'gemini') {
    return '';
  }
  if (activeTool === 'codex' || activeTool === 'opencode') {
    return stringOrEmpty(preset.endpoints.openai_base_url);
  }
  return '';
}

function sanitizedTemplate(template: Record<string, any> | undefined) {
  const output: Record<string, any> = {};
  Object.entries(template || {}).forEach(([key, value]) => {
    const lower = key.toLowerCase();
    if (INSTANCE_KEYS.has(key)) return;
    if (
      lower.includes('key') ||
      lower.includes('token') ||
      lower.includes('secret') ||
      lower.includes('password') ||
      lower.includes('auth')
    ) {
      return;
    }
    output[key] = value;
  });
  return output;
}

export function applyProviderPresetToDraft(
  draft: ProviderPresetDraft,
  preset: ServiceProviderPresetRecord,
  activeTool: PresetTool | string,
): ProviderPresetDraft {
  const preserved = {
    id: draft.id,
    tool: draft.tool,
    api_key: draft.api_key,
    code: draft.code,
    provider_key: draft.provider_key,
    is_enabled: draft.is_enabled,
    env_managed: draft.env_managed,
    favorite_at: draft.favorite_at,
    history: draft.history,
    fetched_models: draft.fetched_models,
  };
  const endpoint = endpointForPreset(preset, activeTool, draft);
  const next: ProviderPresetDraft = {
    ...draft,
    ...sanitizedTemplate(preset.template),
    name: preset.name || draft.name,
    icon: preset.icon || draft.icon,
    ...preserved,
  };

  if (endpoint) {
    next.base_url = endpoint;
    if (activeTool === 'opencode') {
      next.options = {
        ...(next.options || {}),
        apiKey: next.api_key || '',
        baseURL: endpoint,
      };
    }
  }

  return next;
}
