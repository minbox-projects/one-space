import { v4 as uuidv4 } from 'uuid';

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

export type ClaudePresetModelMapping = {
  family: string;
  display_name: string;
  upstream_model: string;
  supports_1m?: boolean;
  supported_capabilities?: string[];
};

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

export type ProviderCopySource = Record<string, any> & {
  id: string;
  name: string;
  tool: string;
};

export type ProviderCopyDraft = Record<string, any> & {
  id: string;
  name: string;
  tool: string;
  provider_key: string;
  code: string;
};

const PROVIDER_COPY_INSTANCE_KEYS = new Set([
  'id',
  'provider_key',
  'code',
  'is_enabled',
  'is_active',
  'active',
  'env_managed',
  'favorite_at',
  'history',
  'fetched_models',
  'created_at',
  'updated_at',
  'last_used_at',
]);

function isSensitiveProviderKey(key: string) {
  const lower = key.toLowerCase();
  return (
    lower.includes('key') ||
    lower.includes('token') ||
    lower.includes('secret') ||
    lower.includes('password') ||
    lower.includes('auth')
  );
}

function sanitizeProviderCopyValue(value: unknown): unknown {
  if (Array.isArray(value)) {
    return value.map(sanitizeProviderCopyValue);
  }
  if (!value || typeof value !== 'object') {
    return value;
  }
  return Object.fromEntries(
    Object.entries(value).flatMap(([key, nestedValue]) =>
      isSensitiveProviderKey(key) ? [] : [[key, sanitizeProviderCopyValue(nestedValue)]],
    ),
  );
}

export function createProviderCopyDraft(source: ProviderCopySource): ProviderCopyDraft {
  const copied = Object.fromEntries(
    Object.entries(source).flatMap(([key, value]) =>
      PROVIDER_COPY_INSTANCE_KEYS.has(key.toLowerCase()) || isSensitiveProviderKey(key)
        ? []
        : [[key, sanitizeProviderCopyValue(value)]],
    ),
  );

  return {
    ...copied,
    id: uuidv4(),
    provider_key: `copy_${uuidv4().replaceAll('-', '')}`,
    code: `copy-${uuidv4()}`,
    name: `${source.name} 副本`,
  } as ProviderCopyDraft;
}

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

const CLAUDE_PRESET_TEMPLATE_KEYS = new Set([
  'claude_default_model',
  'claude_reasoning_effort',
  'claude_model_mappings',
]);

function stringOrEmpty(value: unknown) {
  return typeof value === 'string' ? value.trim() : '';
}

function normalizeClaudeModelMapping(mapping: Partial<ClaudePresetModelMapping>): ClaudePresetModelMapping {
  return {
    family: stringOrEmpty(mapping.family),
    display_name: stringOrEmpty(mapping.display_name),
    upstream_model: stringOrEmpty(mapping.upstream_model),
    supports_1m: !!mapping.supports_1m,
    supported_capabilities: Array.isArray(mapping.supported_capabilities)
      ? mapping.supported_capabilities
          .map((value) => stringOrEmpty(value))
          .filter((value) => value.length > 0)
      : undefined,
  };
}

function hasClaudeMappingValue(mapping: ClaudePresetModelMapping) {
  return (
    mapping.upstream_model.length > 0 ||
    mapping.supports_1m === true ||
    (mapping.supported_capabilities || []).length > 0
  );
}

export function normalizeClaudePresetTemplate(template: Record<string, any> | undefined) {
  const output: Record<string, any> = {};
  const defaultModel = stringOrEmpty(template?.claude_default_model);
  const reasoningEffort = stringOrEmpty(template?.claude_reasoning_effort);
  if (defaultModel) {
    output.claude_default_model = defaultModel;
  }
  if (reasoningEffort) {
    output.claude_reasoning_effort = reasoningEffort;
  }
  if (Array.isArray(template?.claude_model_mappings)) {
    const mappings = template.claude_model_mappings
      .filter((mapping: unknown): mapping is Partial<ClaudePresetModelMapping> =>
        !!mapping && typeof mapping === 'object',
      )
      .map((mapping: Partial<ClaudePresetModelMapping>) => normalizeClaudeModelMapping(mapping))
      .filter((mapping: ClaudePresetModelMapping) => mapping.family.length > 0)
      .filter(hasClaudeMappingValue);
    if (mappings.length > 0) {
      output.claude_model_mappings = mappings;
    }
  }
  return output;
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
    if (INSTANCE_KEYS.has(key)) return;
    if (isSensitiveProviderKey(key)) {
      return;
    }
    output[key] = value;
  });
  return output;
}

function templateForTool(template: Record<string, any>, activeTool: PresetTool | string) {
  const output = { ...template };
  for (const key of CLAUDE_PRESET_TEMPLATE_KEYS) {
    delete output[key];
  }
  if (activeTool === 'claude') {
    Object.assign(output, normalizeClaudePresetTemplate(template));
  }
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
  const template = templateForTool(sanitizedTemplate(preset.template), activeTool);
  const next: ProviderPresetDraft = {
    ...draft,
    ...template,
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
