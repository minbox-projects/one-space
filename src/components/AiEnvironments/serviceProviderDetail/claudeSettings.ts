export interface ClaudeModelMapping {
  family: string;
  display_name: string;
  upstream_model: string;
  supports_1m?: boolean;
  supported_capabilities?: string[];
}

export const AUTH_ENV_OPTIONS = ['ANTHROPIC_API_KEY', 'ANTHROPIC_AUTH_TOKEN'];
export const CLAUDE_TOGGLE_FIELDS = [
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
  return typeof provider?.claude_reasoning_effort === 'string' && provider.claude_reasoning_effort.length > 0
    ? provider.claude_reasoning_effort
    : undefined;
}

function resolveClaudeDefaultModel(provider: any) {
  const raw = typeof provider?.claude_default_model === 'string' && provider.claude_default_model.length > 0
    ? provider.claude_default_model
    : typeof provider?.model === 'string' && provider.model.length > 0
      ? provider.model
      : undefined;
  return raw?.trim() || undefined;
}

export function fallbackProtocolRouterBaseUrl(providerId?: string) {
  return `http://127.0.0.1:17687/protocol-router/service-providers/${providerId || '{id}'}/anthropic`;
}

export function buildClaudeSettingsJson(provider: any, protocolRouterBaseUrl?: string) {
  const env: Record<string, string> = {};
  const apiFormat = provider?.claude_api_format || 'anthropic_messages';
  const authKey = provider?.claude_auth_env_key || 'ANTHROPIC_API_KEY';
  const apiKey = provider?.api_key || '';
  const defaultModel = resolveClaudeDefaultModel(provider);

  if (apiFormat === 'anthropic_messages') {
    env[authKey] = apiKey;
    if (provider?.base_url) {
      env.ANTHROPIC_BASE_URL = provider.base_url;
    }
  } else {
    env.ANTHROPIC_API_KEY = '{protocol-router-token}';
    env.ANTHROPIC_BASE_URL = protocolRouterBaseUrl || fallbackProtocolRouterBaseUrl(provider?.id);
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

  if (defaultModel) {
    env.ANTHROPIC_MODEL = defaultModel;
  }

  const effort = resolveClaudeEffectiveEffort(provider);
  if (effort) {
    env.CLAUDE_CODE_EFFORT_LEVEL = effort;
  }

  if (provider?.claude_enable_tool_search) {
    env.ENABLE_TOOL_SEARCH = 'true';
  }

  const settings: Record<string, any> = {
    model: defaultModel,
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
